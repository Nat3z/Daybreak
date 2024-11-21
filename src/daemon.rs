pub mod daemonhandler {
    use std::{borrow::BorrowMut, collections::LinkedList, fs, io::{Read, Write}, os::unix::net::{UnixListener, UnixStream}, path::Path, process::exit, sync::{Arc, Mutex}, thread};
    use crate::{daemon::daemonhandler, robot::robotmanager::{run_mode::{Mode, RunMode}, Robot}};
    use protobuf::{EnumOrUnknown, SpecialFields};
    use ssh2::Session;
    pub enum MsgDaemonType {
        Upload = 1,
        Connect = 2,
        Run = 3,
        QueryDevices = 4,
        Download = 5,
        InputListener = 6,
        Kill = 255
    }
    pub fn query_message_daemon_type(message: &Vec<u8>) -> Option<MsgDaemonType> {
        let message_type = message[0];
        return match message_type {
            1 => Some(MsgDaemonType::Upload),
            2 => Some(MsgDaemonType::Connect),
            3 => Some(MsgDaemonType::Run),
            4 => Some(MsgDaemonType::QueryDevices),
            5 => Some(MsgDaemonType::Download),
            6 => Some(MsgDaemonType::InputListener),
            255 => Some(MsgDaemonType::Kill),
            _ => None
        }
    }

    // create an event queue static variable
    pub fn main_d() {
        let temp_dir = std::env::temp_dir().into_os_string().into_string().unwrap();
        let listener = UnixListener::bind(format!("{}/daybreak.sock", temp_dir));
        if listener.is_err() {
            println!("Failed to bind to socket.");
            exit(1);
        }
        let listener = listener.unwrap();

        println!("[Daemon] Listening on {}/daybreak.sock", temp_dir);

        let mut robot: Arc<Option<Arc<Robot>>> = Arc::new(None);
        let mut robot_socket: Arc<Mutex<Option<UnixStream>>> = Arc::new(Mutex::new(None));
        let mut ip_addr: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let mut robot_type: Arc<Mutex<Option<u8>>> = Arc::new(Mutex::new(None));
        fn input_listener(socket: Arc<Mutex<UnixStream>>, robot_socket: Arc<Mutex<Option<UnixStream>>>) {
            loop {
                let mut buffer = [0; 1];
                let read = socket.lock().unwrap().read(&mut buffer);
                if read.is_err() {
                    println!("[Daemon @Run] Failed to read from socket.");
                    break;
                }
                if buffer[0] == 4 {
                    // socket.lock().unwrap().set_nonblocking(false).unwrap();
                    // let _dawn_read = robot_socket_clone.lock().unwrap().as_ref().unwrap().set_nonblocking(false);
                    println!("[Daemon @Run] Received message from client. Ending loop.");
                    break;
                }

                if buffer[0] == 5 {
                    let mut length_bytes = [0u8; 2];
                    let _length_read = socket.lock().unwrap().read_exact(&mut length_bytes);
                    if _length_read.is_err() {
                        println!("[Daemon @Run] Failed to read length bytes.");
                        continue;
                    }

                    // Convert length bytes to u16 (little endian)
                    let msg_length = u16::from_le_bytes(length_bytes) as usize;

                    // Read payload of specified length
                    let mut payload = vec![0u8; msg_length];
                    let _payload_read = socket.lock().unwrap().read_exact(&mut payload);
                    if _payload_read.is_err() {
                        continue;
                    }

                    robot_socket.lock().unwrap().as_ref().unwrap().write(&[5]).unwrap();
                    robot_socket.lock().unwrap().as_ref().unwrap().write(&length_bytes).unwrap();
                    robot_socket.lock().unwrap().as_ref().unwrap().write(&payload).unwrap();
                    robot_socket.lock().unwrap().as_ref().unwrap().flush().unwrap();
                }
            }

            println!("[Daemon @Run] Input loop ended.");
            // signal the robot to stop
            robot_socket.lock().unwrap().as_ref().unwrap().write(&[2]).unwrap();
            robot_socket.lock().unwrap().as_ref().unwrap().flush().unwrap();
        }
        loop {
            match listener.accept() {
                Ok((socket, addr)) => {
                    let socket = Arc::new(Mutex::new(socket));
                    let robot_socket_clone = Arc::clone(&robot_socket);
                    // handle the connection
                    // println!("[Daemon] Accepted connection from {:?}", addr);
                    // read the message
                    let mut buffer = [0; 1];
                    let _dawn_read = socket.lock().unwrap().read(&mut buffer);
                    if _dawn_read.is_err() {
                        println!("[Daemon] Failed to read from socket.");
                        exit(1);
                    }

                    let message_type = query_message_daemon_type(&buffer.to_vec());
                    if message_type.is_none() {
                        println!("[Daemon] Unknown message type: {:?}", buffer[0]);
                        continue;
                    }

                    match message_type.unwrap() {
                        MsgDaemonType::Kill => {
                            println!("[Daemon] Received kill message. Gracefully exiting.");
                            // delete the socket file
                            let _ = std::fs::remove_file(format!("{}/daybreak.sock", temp_dir));
                            println!("[Daemon] Deleted socket file.");

                            let _ = socket.lock().unwrap().write(&[200]);
                            let _ = socket.lock().unwrap().flush();
                            exit(0);
                        },
                        MsgDaemonType::Connect => {
                            let mut buf_robo = [0; 1];
                            let _ = socket.lock().unwrap().read(&mut buf_robo);
                            let mut buf = [0; 15];
                            println!("[Daemon] Received connect message.");
                            let _ = socket.lock().unwrap().read(&mut buf);
                            let ip = String::from_utf8(buf.to_vec()).unwrap();
                            let ip = ip.trim_matches(char::from(0));
                            let ip = ip.trim();
                            println!("[Daemon] Received IP: {:?}", ip);
                            let _ = socket.lock().unwrap().write(&[1]);
                            let _ = socket.lock().unwrap().flush();
                            ip_addr = Arc::new(Mutex::new(Some(ip.to_string())));
                            robot = Arc::new(Some(Arc::new(Robot {
                                // event_queue: LinkedList::new()
                            })));

                            let state = robot.as_ref().clone().unwrap().connect(&ip);
                            if state == 200 {
                                println!("[Daemon] Successfully connected to robot. Connecting to robot socket.");
                                let mut robot_socket_temp = UnixStream::connect(format!("{}/daybreak.robot.sock", temp_dir));
                                println!("Robot Type: {}", buf_robo[0]);
                                robot_type = Arc::new(Mutex::new(Some(buf_robo[0])));
                                // run multiple attempts 
                                for _ in 0..5 {
                                    if robot_socket_temp.is_ok() {
                                        break;
                                    }
                                    robot_socket_temp = UnixStream::connect(format!("{}/daybreak.robot.sock", temp_dir));
                                    // wait for 1 second
                                    thread::sleep(std::time::Duration::from_secs(1));
                                }
                                if robot_socket_temp.is_err() {
                                    println!("[Daemon] Failed to connect to robot socket.");
                                    continue;
                                }
                                robot_socket = Arc::new(Mutex::new(Some(robot_socket_temp.unwrap())));
                            } else {
                                println!("[Daemon] Failed to connect to robot.");
                            }
                            let _ = socket.lock().unwrap().write(&[state]);
                            let _ = socket.lock().unwrap().flush();
                        },
                        MsgDaemonType::Download => {
                            println!("[Daemon] Download event caught!");
                            let mut buffer = [0; 1024];
                            let _dawn_read = socket.lock().unwrap().read(&mut buffer);
                            if _dawn_read.is_err() {
                                println!("[Daemon] Failed to read from socket.");
                                continue;
                            }

                            if robot_type.lock().unwrap().is_none() {
                                println!("[Daemon @Download] Unknown robot type.");
                                let _ = socket.lock().unwrap().write(&[50]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }

                            if robot_socket.lock().is_err() {
                                println!("[Daemon @Download] No available Robot.");
                                let _ = socket.lock().unwrap().write(&[50]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }

                            // read the cwd
                            let payload_parts = String::from_utf8(buffer.to_vec());
                            if payload_parts.is_err() {
                                continue;
                            }
                            let payload_parts = payload_parts.unwrap();
                            let payload_parts = payload_parts.trim_matches(char::from(0));
                            let payload_parts = payload_parts.trim();
                            let payload_parts = payload_parts.split(char::from(0)).collect::<Vec<&str>>();
                            if payload_parts.len() <= 1 {
                                continue;
                            }
                            let cwd = payload_parts[0];
                            let file_path = payload_parts[1];
                            println!("[Daemon @Download] Received file path: {:?}", file_path);
                            println!("[Daemon @Download] CWD: {:?}", cwd);

                            // combine the cwd and the file path to get the full path
                            let full_path = format!("{}/{}", cwd, file_path);
                            let full_path = full_path.as_str();
                            println!("[Daemon @Download] Full path: {:?}", full_path);
                            let file_path = std::path::Path::new(full_path);
                            // if the file exists and was requested, send a 200
                            let _ = socket.lock().unwrap().write(&[200]);
                            let _ = socket.lock().unwrap().flush();
                            // connect over ssh
                            let tcp = std::net::TcpStream::connect(
                                format!("{}:22",
                                    ip_addr.lock().unwrap().as_ref().unwrap()
                                )
                            );
                            if tcp.is_err() {
                                println!("[Daemon @Upload] Failed to connect to IP address.");
                                let _ = socket.lock().unwrap().write(&[101]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            let tcp = tcp.unwrap();
                            let mut sess = Session::new().unwrap();
                            sess.set_tcp_stream(tcp);
                            sess.handshake().unwrap();
                            let password_pair = if robot_type.lock().unwrap().unwrap() == 2 {
                                vec!["pi", "raspberry"]
                            } else {
                                vec!["ubuntu", "potato"]
                            };
                            let worked = sess.userauth_password(password_pair[0], password_pair[1]);
                            println!("{:?}", password_pair);
                            if worked.is_err() {
                                println!("[Daemon @Download] Failed to authenticate.");
                                let _ = socket.lock().unwrap().write(&[101]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            let remote_file = sess.scp_recv(Path::new("/home/pi/runtime/executor/studentcode.py"));

                            if remote_file.is_err() {
                                println!("[Daemon @Download] Failed to get file.");
                                let _ = socket.lock().unwrap().write(&[100]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            let mut remote_file = remote_file.unwrap();
                            println!("Path: {:?}", file_path);
                            let file = std::fs::File::create(file_path);
                            if file.is_err() {
                                println!("[Daemon @Upload] Failed to open local file.");
                                let _ = socket.lock().unwrap().write(&[103]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }

                            let mut file = file.unwrap();
                            let mut buffer = Vec::new();

                            let read = remote_file.0.read_to_end(&mut buffer);

                            if read.is_err() {
                                println!("[Daemon @Upload] Failed to read from file.");
                                continue;
                            }
                            file.write_all(&buffer).unwrap();

                            let _ = socket.lock().unwrap().write(&[200]);
                            let _ = socket.lock().unwrap().flush();
                            // completed upload.
                            println!("[Daemon @Download] File has been downloaded.");
                        },
                        MsgDaemonType::Upload => {
                            let mut buffer = [0; 1024];
                            let _dawn_read = socket.lock().unwrap().read(&mut buffer);
                            if _dawn_read.is_err() {
                                println!("[Daemon] Failed to read from socket.");
                                continue;
                            }

                            if robot_type.lock().unwrap().is_none() {
                                println!("[Daemon @Upload] Unknown robot type.");
                                let _ = socket.lock().unwrap().write(&[50]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }

                            if robot_socket.lock().is_err() {
                                println!("[Daemon @Upload] No available Robot.");
                                let _ = socket.lock().unwrap().write(&[50]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }

                            // read the cwd
                            let payload_parts = String::from_utf8(buffer.to_vec()).unwrap();
                            let payload_parts = payload_parts.trim_matches(char::from(0));
                            let payload_parts = payload_parts.trim();
                            let payload_parts = payload_parts.split(char::from(0)).collect::<Vec<&str>>();
                            if payload_parts.len() < 1 {
                                println!("[Daemon @Upload] Bad file");
                                let _ = socket.lock().unwrap().write(&[50]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            let cwd = payload_parts[0];
                            let file_path = payload_parts[1];
                            println!("[Daemon @Upload] Received file path: {:?}", file_path);
                            println!("[Daemon @Upload] CWD: {:?}", cwd);

                            // combine the cwd and the file path to get the full path
                            let full_path = format!("{}/{}", cwd, file_path);
                            let full_path = full_path.as_str();
                            println!("[Daemon @Upload] Full path: {:?}", full_path);
                            let file_path = std::path::Path::new(full_path);
                            if !file_path.exists() {
                                println!("[Daemon @Upload] File does not exist.");
                                let _ = socket.lock().unwrap().write(&[100]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            // if the file exists and was sent, send a 200
                            let _ = socket.lock().unwrap().write(&[200]);
                            let _ = socket.lock().unwrap().flush();
                            // connect over ssh
                            if ip_addr.lock().unwrap().is_none() {
                                println!("[Daemon @Upload] Failed to connect to IP address.");
                                let _ = socket.lock().unwrap().write(&[101]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            let tcp = std::net::TcpStream::connect(format!("{}:22", ip_addr.lock().unwrap().as_ref().unwrap()));
                            if tcp.is_err() {
                                println!("[Daemon @Upload] Failed to connect to IP address.");
                                let _ = socket.lock().unwrap().write(&[101]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            let tcp = tcp.unwrap();
                            let mut sess = Session::new().unwrap();
                            sess.set_tcp_stream(tcp);
                            sess.handshake().unwrap();
                            let password_pair = if robot_type.lock().unwrap().unwrap() == 2 {
                                vec!["pi", "raspberry"]
                            } else {
                                vec!["ubuntu", "potato"]
                            };
                            let worked = sess.userauth_password(password_pair[0], password_pair[1]);
                            if worked.is_err() {
                                println!("[Daemon @Upload] Failed to authenticate.");
                                let _ = socket.lock().unwrap().write(&[101]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            let remote_file = sess.scp_send(Path::new("/home/pi/runtime/executor/studentcode.py"), 0o644, file_path.metadata().unwrap().len(), None);
                            if remote_file.is_err() {
                                println!("[Daemon @Upload] Failed to send file.");
                                let _ = socket.lock().unwrap().write(&[100]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            let mut remote_file = remote_file.unwrap();
                            let file = std::fs::File::open(file_path);
                            if file.is_err() {
                                println!("[Daemon @Upload] Failed to open local file.");
                                let _ = socket.lock().unwrap().write(&[103]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }

                            let mut file = file.unwrap();
                            let mut buffer = [0; 1024];
                            let mut failed = false;
                            loop {
                                let read = file.read(&mut buffer);
                                if read.is_err() {
                                    println!("[Daemon @Upload] Failed to read from file.");
                                    failed = true;
                                    continue;
                                }
                                let read = read.unwrap();
                                if read == 0 {
                                    break;
                                }
                                let _ = remote_file.write(&buffer[..read]);
                            }

                            if failed {
                                continue;
                            }
                            let _ = remote_file.send_eof();
                            let _ = remote_file.wait_eof();
                            let _ = remote_file.wait_close();

                            let _ = socket.lock().unwrap().write(&[200]);
                            let _ = socket.lock().unwrap().flush();
                            // completed upload.
                            println!("[Daemon @Upload] File has been uploaded.");
                        },
                        MsgDaemonType::Run => {
                            let mut buffer = [0; 1];
                            let _dawn_read = socket.lock().unwrap().read(&mut buffer);
                            if _dawn_read.is_err() {
                                println!("[Daemon] Failed to read from socket.");
                                continue;
                            }
                            let buffer = buffer.to_vec();
                            let run_type: u8 = buffer[0];
                            if robot.is_none() || robot_socket_clone.lock().unwrap().is_none() {
                                println!("[Daemon] No Robot Available.");
                                let _ = socket.lock().unwrap().write(&[100]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            if let Some(ref mut robot_socket) = *robot_socket.lock().unwrap() {
                                robot_socket.write(&[run_type]).unwrap();
                                robot_socket.flush().unwrap();
                            }

                            println!("[Daemon] Sent run message to robot.");
                            // now hold the socket until the robot is done running

                            println!("[Daemon @Run] Starting log loop...");
                            let _ = fs::remove_file(format!("{}/robot.run.txt", temp_dir));

                            let _ = socket.lock().unwrap().write(&[1]);
                            let _ = socket.lock().unwrap().flush();
                            thread::spawn(move || {
                                println!("[Daemon @Run] Waiting for robot to finish running.");
                                input_listener(socket, robot_socket_clone);
                            });
                        },
                        MsgDaemonType::InputListener => {
                            println!("[Daemon @InputListener] Received Input Listening Request...");
                            if robot_socket_clone.lock().unwrap().is_none() {
                                println!("[Daemon @InputListener] Request failed.");
                                let _ = socket.lock().unwrap().write(&[1]);
                                let _ = socket.lock().unwrap().flush();
                                continue;
                            }
                            let _ = socket.lock().unwrap().write(&[2]);
                            let _ = socket.lock().unwrap().flush();
                            thread::spawn(move || {
                                println!("[Daemon @InputListener] Waiting for robot to finish running.");
                                input_listener(socket, robot_socket_clone);
                            });
                        }

                        MsgDaemonType::QueryDevices => {
                            // println!("[Daemon @QueryDevices] Received Query Question...");
                            if robot.is_none() || robot_socket_clone.lock().unwrap().is_none() {
                                let _ = socket.lock().unwrap().write(&[0]);
                                let _ = socket.lock().unwrap().flush();
                                // println!("[Daemon @QueryDevices] No robot available.");
                                continue;
                            }

                            // now with the robot, ask for the devices.
                            let mut buffer = [0;3];
                            // println!("[Daemon @QueryDevices] Fetching devices...");
                            robot_socket_clone.lock().unwrap().as_ref().unwrap().write(&[4]).unwrap();
                            robot_socket_clone.lock().unwrap().as_ref().unwrap().flush().unwrap();
                            robot_socket_clone.lock().unwrap().as_ref().unwrap().read_exact(&mut buffer).unwrap();

                            if buffer[0] != 1 {
                                println!("[Daemon @QueryDevices] Failed to fetch devices.");
                                socket.lock().unwrap().write(&[0]).unwrap();
                                socket.lock().unwrap().flush().unwrap();
                                continue;
                            }

                            // with the 3 byte header, get the length of the message let length_arr: Vec<u8> = vec![(length & 0x00ff) as u8, (length & 0xf00) as u8];
                            let length = (buffer[1] as usize) | ((buffer[2] as usize) << 8);
                            let mut buffer = vec![0; length as usize];
                            robot_socket_clone.lock().unwrap().as_ref().unwrap().read(&mut buffer).unwrap();
                            // println!("[Daemon @QueryDevices] Fetched all devices!");
                            let _ = socket.lock().unwrap().write(&[1]);
                            let _ = socket.lock().unwrap().write(&[(buffer.len() & 0x00ff) as u8]);
                            let _ = socket.lock().unwrap().write(&[((buffer.len() & 0xff00) >> 8) as u8]);
                            let _ = socket.lock().unwrap().write(&buffer);
                            let _ = socket.lock().unwrap().flush();
                            // println!("[Daemon @QueryDevices] Sent Devices Info");
                        },
                        _ => {
                            println!("[Daemon] Unknown message type: {:?}", buffer[0]);
                        }
                    }
                },
                Err(e) => {
                    println!("accept function failed: {:?}", e);
                }
            }
        }
    }
}
