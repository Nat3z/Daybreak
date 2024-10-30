pub mod robotmanager {
    use std::{io::{Read, Write}, net::TcpStream, thread};
    include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
    use device::DevData;
    use protobuf::Message;
    use run_mode::RunMode;
    use text::Text;

    const MESSAGE_SIZE: usize = 3;
    pub enum MsgType {
        RunMode = 0,
        StartPos = 1,
        Log = 2,
        DeviceData = 3,
        Inputs = 5,
        TimeStamps = 6
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
    pub fn exit(code: i32) {
        std::process::exit(code);
    }
    fn main_loop(mut stream: TcpStream) {
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
    pub fn connect(default_host: &str) -> u8 {
        let default_port = 8101;
        
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
            return 100;
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
            return 100;
        }

        println!("[Connection] Connected to Stream!");
        thread::spawn(move || {
            main_loop(stream);
        });

        return 200;
    }

}
