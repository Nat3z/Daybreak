
pub mod daemonhandler {
    use std::{borrow::BorrowMut, collections::LinkedList, io::{Read, Write}, os::unix::net::{UnixListener, UnixStream}, path::Path, process::exit, sync::Arc, thread};
    use crate::{daemon::daemonhandler, robot::robotmanager::{run_mode::{Mode, RunMode}, Robot}};
    use protobuf::{EnumOrUnknown, SpecialFields};
    use ssh2::Session;
    pub enum MsgDaemonType {
        Upload = 1,
        Connect = 2,
        Run = 3,
        Kill = 255
    }
    pub fn query_message_daemon_type(message: &Vec<u8>) -> Option<MsgDaemonType> {
        let message_type = message[0];
        return match message_type {
            1 => Some(MsgDaemonType::Upload),
            2 => Some(MsgDaemonType::Connect),
            3 => Some(MsgDaemonType::Run),
            255 => Some(MsgDaemonType::Kill),
            _ => None
        }
    }

    // create an event queue static variable
    pub fn main_d() {
        let listener = UnixListener::bind("/tmp/daybreak.sock");
        if listener.is_err() {
            println!("Failed to bind to socket.");
            exit(1);
        }
        let listener = listener.unwrap();

        println!("[Daemon] Listening on /tmp/daybreak.sock");

        let mut robot: Arc<Option<Arc<Robot>>> = Arc::new(None);
        let mut robot_socket: Option<UnixStream> = None;
        loop {
            match listener.accept() {
                Ok((mut socket, addr)) => {
                    // handle the connection
                    println!("[Daemon] Accepted connection from {:?}", addr);
                    // read the message
                    let mut buffer = [0; 1];
                    let _dawn_read = socket.read(&mut buffer);
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
                            let _ = std::fs::remove_file("/tmp/daybreak.sock");
                            println!("[Daemon] Deleted socket file.");

                            let _ = socket.write(&[200]);
                            let _ = socket.flush();
                            exit(0);
                        },
                        MsgDaemonType::Connect => {
                            let mut buf = [0; 15];
                            println!("[Daemon] Received connect message.");
                            let _ = socket.read(&mut buf);
                            let ip = String::from_utf8(buf.to_vec()).unwrap();
                            let ip = ip.trim_matches(char::from(0));
                            let ip = ip.trim();
                            println!("[Daemon] Received IP: {:?}", ip);
                            let _ = socket.write(&[1]);
                            let _ = socket.flush();
                            robot = Arc::new(Some(Arc::new(Robot {
                                // event_queue: LinkedList::new()
                            })));

                            let state = robot.as_ref().clone().unwrap().connect(&ip);
                            if state == 200 {
                                println!("[Daemon] Successfully connected to robot. Connecting to robot socket.");
                                let mut robot_socket_temp = UnixStream::connect("/tmp/daybreak.robot.sock");
                                // run multiple attempts 
                                for _ in 0..5 {
                                    if robot_socket_temp.is_ok() {
                                        break;
                                    }
                                    robot_socket_temp = UnixStream::connect("/tmp/daybreak.robot.sock");
                                    // wait for 1 second
                                    thread::sleep(std::time::Duration::from_secs(1));
                                }
                                if robot_socket_temp.is_err() {
                                    println!("[Daemon] Failed to connect to robot socket.");
                                    exit(1);
                                }
                                robot_socket = Some(robot_socket_temp.unwrap());
                            } else {
                                println!("[Daemon] Failed to connect to robot.");
                            }
                            let _ = socket.write(&[state]);
                            let _ = socket.flush();
                        },
                        MsgDaemonType::Upload => {
                            let mut buffer = [0; 1024];
                            let _dawn_read = socket.read(&mut buffer);
                            if _dawn_read.is_err() {
                                println!("[Daemon] Failed to read from socket.");
                                return;
                            }

                            // read the cwd
                            let payload_parts = String::from_utf8(buffer.to_vec()).unwrap();
                            let payload_parts = payload_parts.trim_matches(char::from(0));
                            let payload_parts = payload_parts.trim();

                            let cwd = payload_parts.split(char::from(0)).collect::<Vec<&str>>()[0];
                            let file_path = payload_parts.split(char::from(0)).collect::<Vec<&str>>()[1];
                            let ipaddr = payload_parts.split(char::from(0)).collect::<Vec<&str>>()[2];
                            println!("[Daemon @Upload] Received file path: {:?}", file_path);
                            println!("[Daemon @Upload] CWD: {:?}", cwd);

                            // combine the cwd and the file path to get the full path
                            let full_path = format!("{}/{}", cwd, file_path);
                            let full_path = full_path.as_str();
                            println!("[Daemon @Upload] Full path: {:?}", full_path);
                            let file_path = std::path::Path::new(full_path);
                            if !file_path.exists() {
                                println!("[Daemon @Upload] File does not exist.");
                                let _ = socket.write(&[100]);
                                let _ = socket.flush();
                                continue;
                            }
                            // if the file exists and was sent, send a 200
                            let _ = socket.write(&[200]);
                            let _ = socket.flush();
                            // connect over ssh
                            let tcp = std::net::TcpStream::connect(format!("{}:22", ipaddr.trim()));
                            if tcp.is_err() {
                                println!("[Daemon @Upload] Failed to connect to IP address.");
                                let _ = socket.write(&[101]);
                                let _ = socket.flush();
                                continue;
                            }
                            let tcp = tcp.unwrap();
                            let mut sess = Session::new().unwrap();
                            sess.set_tcp_stream(tcp);
                            sess.handshake().unwrap();
                            let worked = sess.userauth_password("pi", "raspberry");
                            if worked.is_err() {
                                println!("[Daemon @Upload] Failed to authenticate.");
                                let _ = socket.write(&[101]);
                                let _ = socket.flush();
                                continue;
                            }
                            let remote_file = sess.scp_send(Path::new("/home/pi/runtime/executor/student_code.py"), 0o644, file_path.metadata().unwrap().len(), None);
                            if remote_file.is_err() {
                                println!("[Daemon @Upload] Failed to send file.");
                                let _ = socket.write(&[100]);
                                let _ = socket.flush();
                                continue;
                            }
                            let mut remote_file = remote_file.unwrap();
                            let file = std::fs::File::open(file_path);
                            if file.is_err() {
                                println!("[Daemon @Upload] Failed to open local file.");
                                let _ = socket.write(&[103]);
                                let _ = socket.flush();
                                continue;
                            }

                            let mut file = file.unwrap();
                            let mut buffer = [0; 1024];
                            loop {
                                let read = file.read(&mut buffer);
                                if read.is_err() {
                                    println!("[Daemon @Upload] Failed to read from file.");
                                    exit(1);
                                }
                                let read = read.unwrap();
                                if read == 0 {
                                    break;
                                }
                                let _ = remote_file.write(&buffer[..read]);
                            }
                            let _ = remote_file.send_eof();
                            let _ = remote_file.wait_eof();
                            let _ = remote_file.wait_close();

                            let _ = socket.write(&[200]);
                            let _ = socket.flush();
                            // completed upload.
                            println!("[Daemon @Upload] File has been uploaded.");
                        },
                        MsgDaemonType::Run => {
                            let mut buffer = [0; 1];
                            let _dawn_read = socket.read(&mut buffer);
                            if _dawn_read.is_err() {
                                println!("[Daemon] Failed to read from socket.");
                                return;
                            }
                            let buffer = buffer.to_vec();
                            let run_type: u8 = buffer[0];
                            if robot.is_none() || robot_socket.is_none() {
                                println!("[Daemon] No Robot Available.");
                                let _ = socket.write(&[100]);
                                let _ = socket.flush();
                                continue;
                            }
                            if let Some(ref mut socket) = robot_socket {
                                socket.write(&[run_type]).unwrap();
                                socket.flush().unwrap();
                            }

                            println!("[Daemon] Sent run message to robot.");

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
