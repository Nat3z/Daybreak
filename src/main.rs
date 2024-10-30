use protobuf::Message;
include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
use device::DevData;
use run_mode::RunMode;
use signal_hook::{consts::SIGINT, iterator::Signals};
use text::Text;
use std::{collections::HashMap, env, io::{Read, Write}, net::TcpStream, os::unix::net::UnixStream, thread};
use daybreak::daemon::daemonhandler;
// 3 byte message
const MESSAGE_SIZE: usize = 3;

enum MsgType {
    RunMode = 0,
    StartPos = 1,
    Log = 2,
    DeviceData = 3,
    Inputs = 5,
    TimeStamps = 6
}


fn exit(code: i32) {
    std::process::exit(code);
}
fn query_message_type(message: &Vec<u8>) -> Option<MsgType> {
    let message_type = message[0];
    return match message_type {
        0 => Some(MsgType::RunMode),
        1 => Some(MsgType::StartPos),
        2 => Some(MsgType::Log),
        3 => Some(MsgType::DeviceData),
        5 => Some(MsgType::Inputs),
        6 => Some(MsgType::TimeStamps),
        _ => None
    }
}


fn on_shutdown() {
    let mut signals = Signals::new([SIGINT]).unwrap();
    thread::spawn(move || {
        for sig in signals.forever() {
            println!("\n[Shutdown] Received signal {:?}", sig);
            // delete the socket file
            let _ = std::fs::remove_file("/tmp/daybreak.sock");
            println!("[Shutdown] Deleted socket file.");
            exit(1);
        }
    });
}


fn main_loop(stream: &mut TcpStream) {
    loop {
        let mut buffer = [0; MESSAGE_SIZE];
        let _dawn_read = stream.read(&mut buffer);
        if _dawn_read.is_err() {
            continue;
        }
        println!("[MessageHandler] Caught a message!");
        let message = buffer.to_vec();
        let message_type = message[0];

        let msg_type = query_message_type(&message);
        if msg_type.is_none() {
            println!("[MessageHandler] Unknown message type: {:?}", message_type);
            continue;
        }

        match msg_type.unwrap() {
            MsgType::RunMode => {
                let run_mode = RunMode::parse_from_bytes(&message[1..]).unwrap();
                println!("[RunMode] {:?}", run_mode);
            }
            MsgType::StartPos => {
                println!("[StartPos] Unimplemented.");
            }
            MsgType::Log => {
                let log = Text::parse_from_bytes(&message[1..]).unwrap();
                println!("[Log] {:?}", log.payload);
            }
            MsgType::DeviceData => {
                let sensors = DevData::parse_from_bytes(&message[1..]).unwrap().devices;
                println!("[DeviceData] {:?}", sensors);
            }
            MsgType::Inputs => {
                println!("[Inputs] Unsupported");
            }
            MsgType::TimeStamps => {
                continue;
            }
        }

    }

}
fn main() {
    let commands: HashMap<&str, &str> = HashMap::from([
        ("upload", "Upload a file to the robot."),
        ("--connect", "Connect to the Daybreak server."),
        ("--help", "Display this help message."),
        ("--start", "Start the Daybreak daemon."),
        ("--start-force", "Start the Daybreak daemon and remove the socket file if it exists.")
    ]);
    let args: Vec<String> = env::args().collect();
    println!("{:?}", args);
    let args: Vec<String> = if args.len() > 1 {
        if args[0] == "target/debug/daybreak" {
            args[2..].to_vec()
        } else {
            env::args().collect()
        }
    } else {
        println!("Please pass a command.");
        exit(1);
        vec![]
    };

    println!("{:?}", args);
    
    if args.len() >= 1 {
        let mut command = args[0].as_str();

        if command == "--start-force" {
            if std::fs::exists("/tmp/daybreak.sock").unwrap() {
                println!("[Daemon] Socket file already exists. Removing...");
                let _daybreak_removal = std::fs::remove_file("/tmp/daybreak.sock");
                if _daybreak_removal.is_err() {
                    println!("[Daemon] Failed to remove socket file.");
                    exit(1);
                }
            }
            command = "--start";
        }

        match command {
            "--connect" => {
                connect();
            },
            "--help" => {
                println!("Usage: daybreak [OPTION");
                println!("Options:");
                commands.iter().for_each(|(k, v)| {
                    println!("    {}\t\t{}", k, v);
                });
            },
            "--start" => {
                if std::fs::exists("/tmp/daybreak.sock").unwrap() {
                    println!("[Daemon] Socket file already exists. Exiting...");
                    exit(1);
                }
                println!("Starting Daybreak Daemon...");
                on_shutdown();
                daemonhandler::main_d();
            },
            "upload" => {
                // connect to daemon
                let stream = UnixStream::connect("/tmp/daybreak.sock");
                if stream.is_err() {
                    println!("[Upload] Failed to connect to daemon.");
                    exit(1);
                }
                let mut stream = stream.unwrap();
                // send the message '1' for the type of message, then send the file path to upload
                let file_path = args[1].as_str();
                let file_path_bytes = file_path.as_bytes();
                let _ = stream.write(&[1]);
                // write the current working directory
                let _dawn_cwd = stream.write(env::current_dir().unwrap().to_str().unwrap().as_bytes());
                // write a 0 byte to separate the cwd and the file path
                let _ = stream.write(&[0]);
                let _dawn_upload = stream.write(file_path_bytes);

                // now send it
                let _dawn_flush = stream.flush();
                if _dawn_flush.is_err() {
                    println!("[Upload] Failed to flush stream.");
                    exit(1);
                }

                println!("[Upload] Sent file path to daemon.");

                // read the response
                let mut buffer = [0; 1];
                let _dawn_read = stream.read(&mut buffer);

                if _dawn_read.is_err() {
                    println!("[Upload] Failed to read from daemon.");
                    exit(1);
                }
                match buffer[0] {
                    200 => {
                        println!("[Upload] File is now uploading...");
                    },
                    100 => {
                        println!("[Upload] File does not exist.");
                    },
                    _ => {
                        println!("[Upload] Unknown response from daemon.");
                    }
                }
            },
            _ => {
                println!("Unknown command: {:?}", command);
                exit(1);
            }
        }

        exit(0);
    }
    
}

fn connect() {
    let default_port = 8101;
    let default_host = "localhost";
    
    let mut stream = TcpStream::connect(format!("{}:{}", default_host, default_port));

    for i in 0..10 {
        stream = TcpStream::connect(format!("{}:{}", default_host, default_port));
        if stream.is_err() {
            println!("[Connection] Failed to connect to stream. Retrying... {}/10", i);
            // wait 2 seconds before retrying
            std::thread::sleep(std::time::Duration::from_secs(2));
            continue;
        }
        break;
    }
    if stream.is_err() {
        println!("[Connection] Failed to connect to stream.");
        exit(1);
    }
    let mut stream = stream.unwrap();
    // write a 1 bit Uint8Array to the stream
    let buf = [0; 1];
    let _dawn_identify = stream.write(&buf);
    if _dawn_identify.is_err() {
        println!("[Identification] Error writing to stream: {:?}", _dawn_identify);
    }

    let _dawn_connect = stream.flush();
    if _dawn_connect.is_err() {
        println!("[Identification] Error when welcoming myself. {:?}", _dawn_connect);
        exit(1);
    }

    println!("[Connection] Connected to Stream!");
    main_loop(&mut stream);
}
