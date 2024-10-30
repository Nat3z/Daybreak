
pub mod daemonhandler {
    use std::{borrow::BorrowMut, io::{Read, Write}, os::unix::net::UnixListener, process::exit};
    use super::super::robot::robotmanager;
    pub enum MsgDaemonType {
        Upload = 1,
        Connect = 2,
        Kill = 255
    }
    pub fn query_message_daemon_type(message: &Vec<u8>) -> Option<MsgDaemonType> {
        let message_type = message[0];
        return match message_type {
            1 => Some(MsgDaemonType::Upload),
            2 => Some(MsgDaemonType::Connect),
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
                            println!("[Daemon] Received IP: {:?}", ip);
                            let _ = socket.write(&[1]);
                            let _ = socket.flush();
                            let state = robotmanager::connect(ip.as_str());
                            let _ = socket.write(&[state]);
                            let _ = socket.flush();
                        },
                        MsgDaemonType::Upload => {
                            let mut buffer = [0; 1024];
                            let _dawn_read = socket.read(&mut buffer);
                            if _dawn_read.is_err() {
                                println!("[Daemon] Failed to read from socket.");
                                exit(1);
                            }

                            // read the cwd
                            let payload_parts = String::from_utf8(buffer.to_vec()).unwrap();
                            let payload_parts = payload_parts.trim_matches(char::from(0));
                            let payload_parts = payload_parts.trim();

                            let cwd = payload_parts.split(char::from(0)).collect::<Vec<&str>>()[0];
                            let file_path = payload_parts.split(char::from(0)).collect::<Vec<&str>>()[1];
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
