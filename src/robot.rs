pub mod robotmanager {
    use std::{collections::LinkedList, fs, io::{Read, Write}, net::{SocketAddr, TcpStream}, os::unix::net::UnixListener, str::FromStr, sync::Arc, thread, time::Duration};
    include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
    use device::{DevData, Param};
    use protobuf::{EnumOrUnknown, Message, SpecialFields};
    use run_mode::{Mode, RunMode};
    use text::Text;

    use crate::daemon;

    const HEADER_SIZE: usize = 3;
    pub enum MsgType {
        RunMode = 0,
        StartPos = 1,
        Log = 2,
        DeviceData = 3,
        Inputs = 5,
        TimeStamps = 6
    }

    pub enum EventType {
        RobotStart = 1,
        RobotStop = 2,
        RobotAuto = 3,
        RobotGiveDevices = 4
    }
    pub struct Robot {
    }

    impl Robot {
        pub fn query_message_type(&self, message: &Vec<u8>) -> Option<MsgType> {
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
        pub fn query_event_type(&self, message: &Vec<u8>) -> Option<EventType> {
            let message_type = message[0];
            return match message_type {
                1 => Some(EventType::RobotStart),
                2 => Some(EventType::RobotStop),
                3 => Some(EventType::RobotAuto),
                4 => Some(EventType::RobotGiveDevices),
                _ => None
            }
        }

        pub fn exit(code: i32) {
            std::process::exit(code);
        }


        pub fn main_loop(&self, mut stream: TcpStream) {
            if fs::metadata("/tmp/daybreak.robot.sock").is_ok() {
                let _ = fs::remove_file("/tmp/daybreak.robot.sock");
            }
            let listener = UnixListener::bind("/tmp/daybreak.robot.sock");
            if listener.is_err() {
                println!("[Connection] Failed to bind to socket.");
                println!("error: {:?}", listener.err());
                return;
            }
            let listener = listener.unwrap();

            println!("[Connection] Listening on /tmp/daybreak.robot.sock");
            println!("[Connection] Waiting for Daemon to connect...");
            let daemon_socket = listener.accept();
            if daemon_socket.is_err() {
                println!("[Connection] Failed to accept connection from Daemon.");
                return;
            }
            let (mut daemon_socket, _) = daemon_socket.unwrap();
            println!("[Connection] Accepted connection from Daemon.");
            println!("[Connection] Started Main loop.");
            daemon_socket.set_nonblocking(true).unwrap();

            let mut is_running = false;
            let mut recent_dev_data: Option<Vec<u8>> = None;

            let mut leftover_bytes: Vec<u8> = vec![];
            // clear the log file
            loop {
                let mut event_buffer: [u8; 1] = [0; 1];
                let event_received = daemon_socket.read(&mut event_buffer);
                if event_received.is_ok() {
                    println!("Received an event!");
                    let event = event_buffer.to_vec();
                    let event_type = self.query_event_type(&event);
                    if event_type.is_none() {
                        println!("[Event] Unknown event type: {:?}", event[0]);
                        continue;
                    }
                    match event_type.unwrap() {
                        EventType::RobotStart => {
                            let message = self.send_run_mode(&RunMode {
                                mode: EnumOrUnknown::from(Mode::TELEOP),
                                special_fields: SpecialFields::default(),
                            });
                            stream.write(message.concat().as_slice()).unwrap();
                            stream.flush().unwrap();
                            println!("[RunMode] Started Running.");
                            is_running = true;
                        },
                        EventType::RobotStop => {
                            let message = self.send_run_mode(&RunMode {
                                mode: EnumOrUnknown::from(Mode::IDLE),
                                special_fields: SpecialFields::default(),
                            });
                            stream.write(message.concat().as_slice()).unwrap();
                            stream.flush().unwrap();
                            println!("[RunMode] Stopped Running.");
                            is_running = false;
                        },
                        EventType::RobotAuto => {
                            let message = self.send_run_mode(&RunMode {
                                mode: EnumOrUnknown::from(Mode::AUTO),
                                special_fields: SpecialFields::default(),
                            });
                            stream.write(message.concat().as_slice()).unwrap();
                            stream.flush().unwrap();
                            println!("[RunMode] Started Auto.");
                            is_running = true;
                        },
                        EventType::RobotGiveDevices => {
                            if recent_dev_data.as_ref().is_none() {
                                daemon_socket.write(&[0]).unwrap();
                                daemon_socket.flush().unwrap();
                                continue;
                            }
                            daemon_socket.write(&[1]).unwrap();
                            // send the length as well so the daemon knows how much to read
                            let length = recent_dev_data.as_ref().unwrap().len();
                            daemon_socket.write(&[(length & 0x00FF) as u8]).unwrap();           // Low byte
                            daemon_socket.write(&[((length & 0xFF00) >> 8) as u8]).unwrap();    // High byte shifted correctly
                            daemon_socket.write(&recent_dev_data.as_ref().unwrap()).unwrap();
                            daemon_socket.flush().unwrap();
                        }
                    }
                }
                let mut buffer: [u8; HEADER_SIZE] = [0; HEADER_SIZE];
                let _dawn_read = stream.read(&mut buffer);
                if _dawn_read.is_err() {
                    println!("[Connection] Failed to read from stream.");
                    continue;
                }
                // println!("[MessageHandler] Caught a message!");
                let buffer = vec![leftover_bytes.to_vec(), buffer.to_vec()].concat(); 
                let header = buffer.clone();

                if HEADER_SIZE > buffer.len() {
                    // SAVE THE HEADER SO WE KNOW TO READ THE REST OF THE MESSAGE
                    leftover_bytes = buffer;
                    println!("Pushed to leftover stack.");
                    continue;
                }
                let msg_type = self.query_message_type(&header);
                if msg_type.is_none() {
                    println!("[MessageHandler] Unknown message type: {:?}", header[0]);
                    continue;
                }
                let msg_length = (header[2] as usize) << 8 | header[1] as usize;
                let mut buffer: Vec<u8> = vec![0; msg_length as usize];
                let _read_buffer = stream.read_exact(&mut buffer);
                if let Err(_) = _read_buffer {
                    leftover_bytes = vec![header, buffer].concat();
                    println!("Pushed to leftover stack.");
                    continue;
                }
                // remove all 0 bytes trailing the buffer

                let payload = buffer;

                match msg_type.unwrap() {
                    MsgType::RunMode => {
                        let run_mode = RunMode::parse_from_bytes(&payload);
                        if run_mode.is_err() {
                            println!("[RunMode] Failed to parse RunMode: {:?}", run_mode.err());
                            continue;
                        }
                        is_running = run_mode.unwrap().mode == EnumOrUnknown::new(Mode::IDLE);
                    }
                    MsgType::StartPos => {
                        // println!("[StartPos] Unimplemented.");
                    }
                    MsgType::Log => {
                        let log = Text::parse_from_bytes(&payload).unwrap();
                        // println!("[Log] {:?}", log.payload);
                        if is_running {
                            // wriute to a file
                            // create the file if it doesn't exist
                            if fs::metadata("/tmp/robot.run.txt").is_err() {
                                let _ = fs::File::create("/tmp/robot.run.txt");
                            }
                            let file = fs::OpenOptions::new().append(true).open("/tmp/robot.run.txt");
                            if file.is_err() {
                                println!("[Log] Failed to open file: {:?}", file.err());
                                continue;
                            }
                            let mut file = file.unwrap();
                            let _write = file.write_all(log.payload.concat().as_bytes());
                            if _write.is_err() {
                                println!("[Log] Failed to write to file: {:?}", _write.err());
                                continue;
                            }
                            let _flush = file.flush();
                            if _flush.is_err() {
                                println!("[Log] Failed to flush file: {:?}", _flush.err());
                                continue;
                            }
                        }
                    }
                    MsgType::DeviceData => {
                        let sensors = DevData::parse_from_bytes(&payload);
                        if sensors.is_err() {
                            println!("Failed to parse DevData: {:?}", sensors.err());
                            continue;
                        }
                        // println!("Received DevData: {:?}", sensors.as_ref().unwrap());
                        recent_dev_data = Some(sensors.unwrap().write_to_bytes().unwrap());
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

        pub fn send_run_mode(&self, run_mode_data: &RunMode) -> Vec<Vec<u8>> {
            let message = run_mode_data.write_to_bytes().unwrap();
            let message_length = message.len();
            let msg_length_arr: Vec<u8> = vec![(message_length & 0x00ff) as u8, (message_length & 0xf00) as u8];
            let msg_type = MsgType::RunMode;

            let mut buffer: Vec<Vec<u8>> = vec![];
            buffer.push(vec![msg_type as u8]);
            buffer.push(msg_length_arr);
            buffer.push(message);
            return buffer;
        }
        pub fn connect(self: Arc<Self>, default_host: &str) -> u8 {
            let default_port = 8101;
            println!("[Connection] Attempting to connect...");
            let socket = SocketAddr::from_str(format!("{}:{}", default_host, default_port).as_str());
            if socket.is_err() {
                println!("[Connection] Failed to create connection socket.");
                return 100;
            }
            let socket = socket.unwrap();
            let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(5));
            for i in 0..4 {
                stream = TcpStream::connect_timeout(&socket, Duration::from_secs(5));
                if stream.is_err() {
                    println!("[Connection] Failed to connect to stream. Retrying... {}/4", i);
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
            let buf: [u8; 1] = [1];

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
            let robot = Arc::clone(&self);
            thread::spawn(move || robot.main_loop(stream));
            return 200;
        }
    }


}
