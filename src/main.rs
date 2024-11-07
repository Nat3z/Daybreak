use gilrs::{Axis, Button, Event, GamepadId, Gilrs};
use linked_hash_map::LinkedHashMap;
use protobuf::{EnumOrUnknown, Message, SpecialFields};
use signal_hook::{consts::SIGINT, iterator::Signals};
use termion::{input::TermRead, raw::IntoRawMode};
use std::{collections::HashMap, env::{self, temp_dir}, fs, io::{self, stdin, stdout, Read, Write}, net::TcpStream, ops::Index, os::unix::net::UnixStream, sync::{Arc, Mutex}, thread, time::Duration};
use daybreak::{daemon::daemonhandler, keymap::{gamepad_mapped, key_map}, robot::robotmanager::{device::{param::Val, DevData}, input::{Input, Source}}};
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
            let _ = std::fs::remove_file(format!("{}/daybreak.sock", std::env::temp_dir().into_os_string().into_string().unwrap()));
            println!("[Shutdown] Deleted socket file.");
            exit(1);
        }
    });
}

fn main() {
    let temp_dir = std::env::temp_dir().into_os_string().into_string().unwrap();
    let mut commands: LinkedHashMap<&str, &str> = LinkedHashMap::new();
    commands.insert("--connect [IP] [raspberry/potato]", "Connect to Runtime");
    commands.insert("--start", "Start the Daybreak daemon.");
    commands.insert("--start-force", "Start the Daybreak daemon and remove the socket file if it exists.");
    commands.insert("--help", "Display this help message.");
    commands.insert("upload [FILE PATH]", "Upload a file to the robot.");
    commands.insert("download [FILE PATH]", "Downloads the studentcode from the robot.");
    commands.insert("shutdown", "Shutdown the Daybreak daemon.");
    commands.insert("run [auto, teleop, stop]", "Executes code on the robot.");
    commands.insert("input", "Sets the robot to be on generic input listener mode.");
    commands.insert("ls", "Lists all connected devices.");
    commands.insert("    -a", "Attaches to device lister until shutdown.");
    commands.insert("    -t/--frequency [ms]", "Sets the frequency for device listing reload.");
    let args: Vec<String> = env::args().collect();
    fn show_help(commands: &LinkedHashMap<&str, &str>) {
        println!("Usage: daybreak [OPTION]");
        println!("Options:");
        commands.iter().for_each(|(k, v)| {
            println!("    {}\t{}", k, v);
        });
    }
    let args: Vec<String> = if args.len() > 1 {
        if args[0] == "target/debug/daybreak" {
            args[2..].to_vec()
        } else {
            args[1..].to_vec()
        }
    } else {
        vec![]
    };

    if args.len() < 1 {
        println!("Please pass a command.");
        show_help(&commands);
        exit(1);
    }

    let mut command = args[0].as_str();

    if command == "--start-force" {
        if std::fs::exists(format!("{}/daybreak.sock", temp_dir)).unwrap() {
            println!("[Connection] Socket file already exists. Removing...");
            let _daybreak_removal = std::fs::remove_file(format!("{}/daybreak.sock", temp_dir));
            if _daybreak_removal.is_err() {
                println!("[Connection] Failed to remove socket file.");
                exit(1);
            }
        }
        command = "--start";
    }


    fn input_executor(stream: Arc<Mutex<UnixStream>>) {
        let stream_clone = Arc::clone(&stream);
        thread::spawn(move || {
            for sig in Signals::new([SIGINT]).unwrap().forever() {
                println!("\n[Run] Received signal {:?}", sig);
                let stream = stream_clone.lock();
                if stream.is_err() {
                    println!("[Run] Failed to connect to daemon.");
                    exit(1);
                }
                let mut stream = stream.unwrap();
                let _ = stream.write(&[4]);
                let _ = stream.flush();
                println!("[Run] Sent stop message to daemon.");
                exit(0);
            }
        });

        let mut gilrs = Gilrs::new().unwrap();

        // Iterate over all connected gamepads
        for (_id, gamepad) in gilrs.gamepads() {
            println!("{} is {:?}", gamepad.name(), gamepad.power_info());
        }

        thread::spawn(move || {
            let mut active_gamepad: Option<GamepadId> = None;
            let mut button_map: HashMap<Button, bool> = HashMap::new();
            // fill the button map with false
            button_map.insert(Button::DPadDown, false);
            button_map.insert(Button::DPadUp, false);
            button_map.insert(Button::DPadLeft, false);
            button_map.insert(Button::DPadRight, false);
            button_map.insert(Button::South, false);
            button_map.insert(Button::East, false);
            button_map.insert(Button::West, false);
            button_map.insert(Button::North, false);
            button_map.insert(Button::LeftTrigger, false);
            button_map.insert(Button::RightTrigger, false);
            button_map.insert(Button::LeftTrigger2, false);
            button_map.insert(Button::RightTrigger2, false);
            button_map.insert(Button::LeftThumb, false);
            button_map.insert(Button::RightThumb, false);
            button_map.insert(Button::Select, false);
            button_map.insert(Button::Start, false);
            button_map.insert(Button::Mode, false);
            button_map.insert(Button::LeftThumb, false);
            button_map.insert(Button::RightThumb, false);

            loop {
                while let Some(Event { id, event, time, .. }) = gilrs.next_event() {
                    // println!("{:?} New event from {}: {:?}", time, id, event);
                    active_gamepad = Some(id);
                }
                if let Some(gamepad) = active_gamepad.map(|id| gilrs.gamepad(id)) {
                    // check if the button is pressed
                    for button in button_map.clone().keys() {
                        let is_pressed = gamepad.is_pressed(*button);
                        button_map.insert(button.clone(), is_pressed);
                    }

                    let mut bitmap: u64 = 0;
                    // check if the button is pressed
                    for (button, is_pressed) in button_map.clone().iter() {
                        let mapped_index = gamepad_mapped(&button);
                        if *is_pressed {
                            bitmap |= 1 << mapped_index;
                        }
                    }
                    let mut stream = stream.lock().unwrap();
                    stream.write(&[5]).unwrap();
                    // send the length of the message
                    let input = Input {
                        connected: true,
                        buttons: bitmap,
                        axes: vec![
                            gamepad.value(Axis::LeftStickX),
                            gamepad.value(Axis::LeftStickY),
                            gamepad.value(Axis::RightStickX),
                            gamepad.value(Axis::RightStickY)
                        ],
                        source: EnumOrUnknown::new(Source::GAMEPAD),
                        special_fields: SpecialFields::default()
                    };
                    let bytes = input.write_to_bytes().unwrap();
                    stream.write(&[(bytes.len() & 0x00ff) as u8]).unwrap();
                    stream.write(&[((bytes.len() & 0xff00) >> 8) as u8]).unwrap();
                    stream.write(&bytes).unwrap();
                    stream.flush().unwrap();
                    std::thread::sleep(Duration::from_millis(50));
                }

            }
        });
    }

    match command {
        "--connect" => {
            let stream = UnixStream::connect(format!("{}/daybreak.sock", temp_dir));
            if stream.is_err() {
                println!("[Connection] Failed to connect to stream.");
                exit(1);
            }
            if args.len() < 2 {
                println!("[Connection] Please pass an IP address to connect to and (optionally) the robot type.");
                exit(1);
            }

            let ip = args[1].as_str();
            let robot_type = if args.len() >= 3 {
                args[2].as_str()
            } else {
                "potato"
            };

            let mut stream = stream.unwrap();
            let _ = stream.write(&[2]);
            let robot_ip_as_bytes = ip.as_bytes();

            let robot_type = if robot_type.to_lowercase() == "potato" {
                1
            } else if robot_type.to_lowercase() == "raspberry" {
                2
            } else {
                0
            };

            if robot_type == 0 {
                println!("[Connection] Invalid robot type. Valid Types: raspberry/potato");
                exit(1);
            }
            let _ = stream.write(&[robot_type]);
            let _ = stream.write(robot_ip_as_bytes);
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
            println!("Usage: daybreak [OPTION]");
            println!("Options:");
            commands.iter().for_each(|(k, v)| {
                println!("    {}\t{}", k, v);
            });
        },
        "--start" => {
            if std::fs::exists(format!("{}/daybreak.sock", temp_dir)).unwrap() {
                println!("[Daemon] Socket file already exists. Exiting...");
                exit(1);
            }
            println!("Starting Daybreak Daemon...");
            on_shutdown();
            daemonhandler::main_d();
        },
        "ls" => {
            let attach = args.contains(&"-a".to_string()) || args.contains(&"--attach".to_string());
            let mut frequency: u64 = 1000;
            if args.len() > 1 {
                let arg = args.iter().position(|s| s == "-t" || s == "--frequency");
                if arg.is_some() {
                    let argument = args.get(arg.unwrap() + 1);
                    if argument.is_none() {
                        println!("Please pass your frequency in miliseconds.");
                        exit(1);
                    }

                    let argument = argument.unwrap();
                    let freq_from_user = argument.parse::<u64>();
                    if freq_from_user.is_err() {
                        println!("Invalid number.");
                        exit(1);
                    }

                    frequency = freq_from_user.unwrap().clone();
                }
            }

            fn read_devices() {
                for _ in 1..10 {
                    let stream = UnixStream::connect(format!("{}/daybreak.sock", std::env::temp_dir().into_os_string().into_string().unwrap()));
                    if stream.is_err() {
                        println!("[List Devices] Failed to connect to daemon.");
                        exit(1);
                    }

                    let mut stream = stream.unwrap();
                    stream.write(&[4]).unwrap();
                    stream.flush().unwrap();
                    let mut buffer = [0; 3];
                    stream.read(&mut buffer).unwrap();
                    if buffer[0] == 0 {
                        println!("[List Devices] No robot available.");
                        return;
                    }

                    let msg_length = (buffer[2] as usize) << 8 | buffer[1] as usize;
                    let mut buffer = vec![0; msg_length];
                    stream.read_exact(&mut buffer).unwrap();
                    let device_data = DevData::parse_from_bytes(&buffer);
                    if device_data.is_err() {
                        println!("[List Devices] Failed to parse devices list.");
                        println!("{:?}", device_data.err().unwrap());
                        return;
                    }
                    let device_data = device_data.unwrap();
                    // TODO - Work on Parsing Device Data, make it pretty
                    let devices = device_data.devices;

                    if devices.len() == 0 {
                        // println!("No devices available.");
                        continue;
                    }

                    for device in devices {
                        if device.name == "CustomData" {
                            println!("{} (Stopwatch)", device.uid);
                        }
                        else {
                            println!("{} ({})", device.uid, device.name);
                        }
                        for field in device.params {
                            // turn the val into its respective data type
                            let val = field.val.as_ref().unwrap();
                            let val = match val {
                                Val::Bval(val) => {
                                    val.to_string()
                                },
                                Val::Fval(val) => {
                                    val.to_string()
                                },
                                Val::Ival(val) => {
                                    val.to_string()
                                },
                                _ => {
                                    "Unknown".to_string()
                                }
                            };
                            println!("{} - {}", field.name, val);
                        }
                        println!("\n");
                    }
                    break;
                }
            }
            if attach {
                let duration = Duration::from_millis(frequency);
                loop {
                    print!("\x1B[2J\x1B[1;1H");
                    read_devices();
                    std::thread::sleep(duration);
                }
            }
            else {
                read_devices();
            }



        },
        "download" => {
            // connect to daemon
            let stream = UnixStream::connect(format!("{}/daybreak.sock", temp_dir));
            if stream.is_err() {
                println!("[Download] Failed to connect to daemon.");
                exit(1);
            }
            if args.len() < 2 {
                println!("[Download] Please pass a file path to put the file into.");
                println!("Usage: daybreak download [FILE PATH]");
                exit(1);
            }
            let mut stream = stream.unwrap();
            // send the message '1' for the type of message, then send the file path to upload
            let file_path = args[1].as_str();

            let _ = stream.write(&[5]);

            // write the current working directory
            // write a 0 byte to separate the cwd and the file path
            // write the current working directory
            // write a 0 byte to separate the cwd and the file path


            let _dawn_cwd = stream.write(env::current_dir().unwrap().to_str().unwrap().as_bytes());
            let _ = stream.write(&[0]);
            let file_path_bytes = file_path.as_bytes();
            let _dawn_upload = stream.write(file_path_bytes);
            let _dawn_flush = stream.flush();
            if _dawn_flush.is_err() {
                println!("[Download] Failed to flush stream.");
                exit(1);
            }

            println!("[Download] Sent file path to daemon.");

            // read the response
            let mut buffer = [0; 1];
            let _dawn_read = stream.read(&mut buffer);

            if _dawn_read.is_err() {
                println!("[Download] Failed to read from daemon.");
                exit(1);
            }
            match buffer[0] {
                200 => {
                    println!("[Download] File is now downloading...");

                    let mut buffer = [0; 1];
                    let _dawn_read = stream.read(&mut buffer);
                    if _dawn_read.is_err() {
                        println!("[Download] Failed to read from daemon.");
                        exit(1);
                    }

                    if buffer[0] == 200 {
                        println!("[Download] File has been downloaded.");
                    } else {
                        match buffer[0] {
                            100 => {
                                println!("[Download] File does not exist.");
                            },
                            104 => {
                                println!("[Download] Failed to read IP address.");
                            },
                            105 => {
                                println!("[Download] Failed to download file.");
                            },
                            101 => {
                                println!("[Download] Failed to authenticate with ssh.");
                            },
                            102 => {
                                println!("[Download] Failed to connect to ssh.");
                            },
                            103 => {
                                println!("[Download] Failed to read from local file. (Check permissions)");
                            },
                            _ => {
                                println!("[Download] Unknown response from daemon.");
                            }
                        }
                    }
                },
                100 => {
                    println!("[Download] File does not exist.");
                },
                50 => {
                    println!("[Download] No available robot.");
                },
                _ => {
                    println!("[Download] Unknown response from daemon.");
                }
            }
        },
        "upload" => {
            // connect to daemon
            let stream = UnixStream::connect(format!("{}/daybreak.sock", temp_dir));
            if stream.is_err() {
                println!("[Upload] Failed to connect to daemon.");
                exit(1);
            }
            if args.len() < 2 {
                println!("[Upload] Please pass a file path to upload.");
                println!("Usage: daybreak upload [FILE PATH]");
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
            let stream = UnixStream::connect(format!("{}/daybreak.sock", temp_dir));
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
                input_executor(stream_clone);
            });
            stream.lock().unwrap().set_nonblocking(true).unwrap();
            let mut buffer = vec![];
            loop {
                // read from the {TEMP DIR}/robot.run.txt and update the log if there is any new data
                let file = fs::read_to_string(format!("{}/robot.run.txt", temp_dir));
                if file.is_err() {
                    continue;
                }
                let file = file.unwrap();
                if file.len() == 0 {
                    continue;
                }
                let file = file.as_bytes().to_vec();
                // now compare this file with the buffer, if there is new data at the end, then update the buffer, then send the text to console
                if file.len() > buffer.len() {
                    let new_data = &file[buffer.len()..];
                    let new_data = String::from_utf8(new_data.to_vec()).unwrap();
                    print!("{}", new_data);
                    buffer = file;
                }
            }

        },
        "input" => {
            let stream = UnixStream::connect(format!("{}/daybreak.sock", temp_dir));
            if stream.is_err() {
                println!("[Run] Failed to connect to daemon.");
                exit(1);
            }
            let stream = Arc::new(Mutex::new(stream.unwrap()));
            stream.lock().unwrap().write(&[6]).unwrap();
            stream.lock().unwrap().flush().unwrap();

            let mut buffer = [0; 1];
            let read_error = stream.lock().unwrap().read_exact(&mut buffer);

            if read_error.is_err() {
                println!("[Input] Failed to read daemon.");
                exit(1);
            }

            if buffer[0] == 1 {
                println!("[Input] Daemon refused to fulfill request.");
                exit(1);
            }

            println!("[Input] Sent input request message to daemon.");
            println!("[Input] Waiting for response...");

            let stream_clone = Arc::clone(&stream);
            thread::spawn(move || {
                input_executor(stream_clone);
            });
            stream.lock().unwrap().set_nonblocking(true).unwrap();
            println!("[Input] Started input listener.");
        },
        "shutdown" => {
            let stream = UnixStream::connect(format!("{}/daybreak.sock", temp_dir));
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
