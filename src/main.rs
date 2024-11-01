use linked_hash_map::LinkedHashMap;
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::{env, io::{Read, Write}, net::TcpStream, os::unix::net::UnixStream, sync::{Arc, Mutex}, thread};
use daybreak::daemon::daemonhandler;
// 3 byte message

fn exit(code: i32) {
    std::process::exit(code);
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



fn main() {
    let mut commands: LinkedHashMap<&str, &str> = LinkedHashMap::new();
    commands.insert("--connect", "Connect to the Daybreak server.");
    commands.insert("--start", "Start the Daybreak daemon.");
    commands.insert("--start-force", "Start the Daybreak daemon and remove the socket file if it exists.");
    commands.insert("--help", "Display this help message.");
    commands.insert("upload", "Upload a file to the robot.");
    commands.insert("shutdown", "Shutdown the Daybreak daemon.");
    let args: Vec<String> = env::args().collect();
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

    if args.len() < 1 {
        println!("Please pass a command.");
        // show help
        println!("Usage: daybreak [OPTION]");
        println!("Options:");
        commands.iter().for_each(|(k, v)| {
            println!("    {}\t{}", k, v);
        });
        exit(1);
    }

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
            let stream = UnixStream::connect("/tmp/daybreak.sock");
            if stream.is_err() {
                println!("[Connection] Failed to connect to stream.");
                exit(1);
            }
            if args.len() < 2 {
                println!("[Connection] Please pass an IP address to connect to.");
                exit(1);
            }

            let ip = args[1].as_str();

            let mut stream = stream.unwrap();
            let _ = stream.write(&[2]);
            let _ = stream.write(ip.as_bytes());
            let _ = stream.flush();
            println!("[Connection] Sending connection request to daemon...");

            let mut buffer = [0; 1];
            let _dawn_read = stream.read(&mut buffer);
            if _dawn_read.is_err() {
                println!("[Connection] Failed to read from daemon.");
                exit(1);
            }

            if buffer[0] == 1 {
                println!("[Connection] Daemon acknowledged request... Waiting for connection.");

                let _dawn_read = stream.read(&mut buffer);
                if _dawn_read.is_err() {
                    println!("[Connection] Failed to read from daemon.");
                    exit(1);
                }

                if buffer[0] == 200 {
                    println!("[Connection] Successfully connected to Robot.");
                    exit(0);
                } else {
                    println!("[Connection] Failed to connect to Robot.");
                    exit(1);
                }
            } else {
                println!("[Connection] Failed to connect to daemon.");
                exit(1);
            }
        },
        "--help" => {
            println!("Usage: daybreak [OPTION");
            println!("Options:");
            commands.iter().for_each(|(k, v)| {
                println!("    {}\t{}", k, v);
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
            if args.len() < 3 {
                println!("[Upload] Please pass a file path to upload.");
                println!("Usage: daybreak upload [FILE PATH] [IP ADDRESS]");
                exit(1);
            }
            let mut stream = stream.unwrap();
            // send the message '1' for the type of message, then send the file path to upload
            let file_path = args[1].as_str();
            let file_path_bytes = file_path.as_bytes();

            let ipaddr = args[2].as_str();
            let _ = stream.write(&[1]);

            // write the current working directory
            let _dawn_cwd = stream.write(env::current_dir().unwrap().to_str().unwrap().as_bytes());
            // write a 0 byte to separate the cwd and the file path
            let _ = stream.write(&[0]);
            let _dawn_upload = stream.write(file_path_bytes);
            let _ = stream.write(&[0]);
            let _ = stream.write(ipaddr.as_bytes());
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

                    let mut buffer = [0; 1];
                    let _dawn_read = stream.read(&mut buffer);
                    if _dawn_read.is_err() {
                        println!("[Upload] Failed to read from daemon.");
                        exit(1);
                    }

                    if buffer[0] == 200 {
                        println!("[Upload] File has been uploaded.");
                    } else {
                        match buffer[0] {
                            100 => {
                                println!("[Upload] File does not exist.");
                            },
                            104 => {
                                println!("[Upload] Failed to read IP address.");
                            },
                            105 => {
                                println!("[Upload] Failed to upload file.");
                            },
                            101 => {
                                println!("[Upload] Failed to authenticate with ssh.");
                            },
                            102 => {
                                println!("[Upload] Failed to connect to ssh.");
                            },
                            103 => {
                                println!("[Upload] Failed to read from local file. (Check permissions)");
                            },
                            _ => {
                                println!("[Upload] Unknown response from daemon.");
                            }
                        }
                    }
                },
                100 => {
                    println!("[Upload] File does not exist.");
                },
                _ => {
                    println!("[Upload] Unknown response from daemon.");
                }
            }
        },
        "run" => {
            if args.len() < 2 {
                println!("[Run] Please pass the type of run mode. (auto, teleop, stop)");
                exit(1);
            }

            let run_mode = args[1].as_str();
            let run_mode = match run_mode {
                "auto" => 3,
                "teleop" => 1,
                "stop" => 2,
                _ => {
                    println!("[Run] Unknown run mode: {:?}", run_mode);
                    exit(1);
                    0
                }
            };
            let stream = UnixStream::connect("/tmp/daybreak.sock");
            if stream.is_err() {
                println!("[Run] Failed to connect to daemon.");
                exit(1);
            }
            let stream = Arc::new(Mutex::new(stream.unwrap()));
            stream.lock().unwrap().write(&[3]).unwrap();
            stream.lock().unwrap().write(&[run_mode]).unwrap();
            let _ = stream.lock().unwrap().flush();
            println!("[Run] Sent run message to daemon.");
            println!("[Run] Waiting for response...");
            if run_mode == 2 {
                println!("[Run] Completed exit.");
                exit(0);
            }
            let stream_clone = Arc::clone(&stream);
            thread::spawn(move || {
                for sig in Signals::new([SIGINT]).unwrap().forever() {
                    println!("\n[Run] Received signal {:?}", sig);
                    stream_clone.lock().unwrap().write(&[4]).unwrap();
                    stream_clone.lock().unwrap().flush().unwrap();
                    println!("[Run] Sent stop message to daemon.");
                    exit(0);
                }
            });
            stream.lock().unwrap().set_nonblocking(true).unwrap();
            loop {
                let mut buffer = [0; 3608];
                let _dawn_read = stream.lock().unwrap().read(&mut buffer);
                if _dawn_read.is_err() {
                    continue;
                }
                let buffer = buffer.to_vec();
                // turn buffer into text array
                let buffer: Vec<u8> = buffer.iter().filter(|x| **x != 0).map(|x| *x).collect();
                let buffer: Vec<char> = buffer.iter().map(|x| *x as char).collect();
                let buffer: String = buffer.iter().collect();
                println!("{}", buffer);
            }

        }
        "shutdown" => {
            let stream = UnixStream::connect("/tmp/daybreak.sock");
            if stream.is_err() {
                println!("[Shutdown] Failed to connect to daemon.");
                exit(1);
            }
            let mut stream = stream.unwrap();
            let _ = stream.write(&[255]);
            let _ = stream.flush();
            println!("[Shutdown] Sent shutdown message to daemon.");
            let mut buffer = [0; 1];
            let _dawn_read = stream.read(&mut buffer);
            if _dawn_read.is_err() {
                println!("[Shutdown] Failed to read from daemon.");
                exit(1);
            }
            if buffer[0] == 200 {
                println!("[Shutdown] Daemon has shutdown.");
            } else {
                println!("[Shutdown] Daemon failed to shutdown.");
            }
        }
        _ => {
            println!("Unknown command: {:?}", command);
            exit(1);
        }
    }
    
}
