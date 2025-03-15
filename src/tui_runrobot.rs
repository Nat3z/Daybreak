pub mod run_robot_tui {
    use std::{
        collections::HashMap,
        fs,
        io::{Read, Write},
        net::TcpStream,
        os::unix::net::UnixStream,
        process::exit,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::{channel, Receiver},
            Arc, Mutex,
        },
        thread,
        time::Duration,
    };

    use crossterm::event;
    use gilrs::{Axis, Button, Event, GamepadId, Gilrs};
    use protobuf::{EnumOrUnknown, Message, SpecialFields};
    use ratatui::{
        layout::{Constraint, Layout},
        style::{Style, Stylize},
        text::Line,
        widgets::{Block, List, ListItem, ListState},
    };
    use signal_hook::{consts::SIGINT, iterator::Signals};
    use Constraint::Percentage;

    use crate::{
        keymap::gamepad_mapped,
        robot::robotmanager::input::{Input, Source},
        tui::tui::App,
        tui_readdevices::read_devices_tui::read_devices,
    };
    pub fn tui(stream: Arc<Mutex<UnixStream>>) {
        println!("Starting TUI...");
        let devices_string: Arc<Mutex<String>> =
            Arc::new(Mutex::new(String::from("Disconnected from Robot")));
        let terminal_string = Arc::new(Mutex::new(String::new()));
        let selected_pane = Arc::new(Mutex::new(0));

        let stream = Arc::clone(&stream);

        println!("Initialized streams...");

        println!("Starting executor..");
        let is_robot_running = Arc::new(Mutex::new(false));

        println!("Starting log looker...");
        let terminal_string_clone = Arc::clone(&terminal_string);
        let temp_dir = std::env::temp_dir().into_os_string().into_string().unwrap();
        thread::spawn(move || {
            let mut buffer = vec![];
            loop {
                // read from the {TEMP DIR}/robot.run.txt and update the log if there is any new data
                let file = fs::read_to_string(format!("{}/robot.run.txt", temp_dir));
                if file.is_err() {
                    buffer = vec![];
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
                    terminal_string_clone.lock().unwrap().push_str(&new_data);
                    buffer = file;
                }
            }
        });

        let mut terminal = ratatui::init();
        let app_devices_pane = Arc::new(Mutex::new(App::new()));
        let app_terminal_pane = Arc::new(Mutex::new(App::new()));

        let terminal_string_clone = Arc::clone(&terminal_string);
        let stream_clone = Arc::clone(&stream);

        let app_devices_clone = Arc::clone(&app_devices_pane);
        let app_terminal_clone = Arc::clone(&app_terminal_pane);

        println!("Initializing event listener...");
        let is_robot_running_clone = Arc::clone(&is_robot_running);
        let temp_dir = std::env::temp_dir().into_os_string().into_string().unwrap();

        let unripe_stream = Arc::new(Mutex::new(false));

        // update unripe streams
        let unripe_stream_clone = Arc::clone(&unripe_stream);
        thread::spawn(move || {
            loop {
                if unripe_stream_clone.lock().unwrap().eq(&true) {
                    *unripe_stream_clone.lock().unwrap() = false;
                    let stream_change = UnixStream::connect(format!("{}/daybreak.sock", temp_dir));
                    if stream_change.is_err() {
                        terminal_string_clone
                            .lock()
                            .unwrap()
                            .push_str("Failed to prime new stream.\n");
                        continue;
                    }
                    *stream_clone.lock().unwrap() = stream_change.unwrap();
                    // terminal_string_clone.lock().unwrap().push_str("Stream primed!\n");
                }
            }
        });

        let terminal_string_clone = Arc::clone(&terminal_string);
        let stream_clone = Arc::clone(&stream);

        let atomic_break_loop = Arc::new(AtomicBool::new(false));

        let selected_pane_clone = Arc::clone(&selected_pane);
        let devices_string_clone = Arc::clone(&devices_string);
        thread::spawn(move || {
            loop {
                match event::read().unwrap() {
                    event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Esc,
                        ..
                    })
                    | event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Char('q'),
                        ..
                    }) => {
                        ratatui::restore();
                        let mut stream = stream_clone.lock().unwrap();
                        let _ = stream.write(&[4]);
                        let _ = stream.flush();
                        exit(0);
                    }
                    event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Char('s'),
                        ..
                    }) => {
                        if *is_robot_running_clone.lock().unwrap() {
                            continue;
                        }

                        atomic_break_loop.store(false, Ordering::Release);
                        // start in teleop
                        let mut stream = stream_clone.lock().unwrap();
                        let _ = stream.write(&[3]);
                        let _ = stream.write(&[1]);
                        let _ = stream.flush();
                        terminal_string_clone
                            .lock()
                            .unwrap()
                            .push_str("Starting in teleop mode\n");

                        let gilrs = Gilrs::new().unwrap();
                        for (_id, gamepad) in gilrs.gamepads() {
                            terminal_string_clone.lock().unwrap().push_str(
                                format!("{} is {:?}\n", gamepad.name(), gamepad.power_info())
                                    .as_str(),
                            );
                        }

                        if gilrs.gamepads().count() == 0 {
                            terminal_string_clone
                                .lock()
                                .unwrap()
                                .push_str("No gamepads available.\n");
                        }
                        *is_robot_running_clone.lock().unwrap() = true;
                        let stream_clone = Arc::clone(&stream_clone);
                        let atomic_break_loop = Arc::clone(&atomic_break_loop);
                        thread::spawn(move || {
                            input_executor(Arc::clone(&stream_clone), false, atomic_break_loop)
                        });
                    }
                    event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Char('i'),
                        ..
                    }) => {
                        if *is_robot_running_clone.lock().unwrap() {
                            continue;
                        }

                        atomic_break_loop.store(false, Ordering::Release);
                        // start in teleop
                        let mut stream = stream_clone.lock().unwrap();
                        let _ = stream.write(&[6]);
                        let _ = stream.write(&[1]);
                        let _ = stream.flush();
                        terminal_string_clone
                            .lock()
                            .unwrap()
                            .push_str("Starting in input mode\n");

                        let gilrs = Gilrs::new().unwrap();
                        for (_id, gamepad) in gilrs.gamepads() {
                            terminal_string_clone.lock().unwrap().push_str(
                                format!("{} is {:?}\n", gamepad.name(), gamepad.power_info())
                                    .as_str(),
                            );
                        }

                        if gilrs.gamepads().count() == 0 {
                            terminal_string_clone
                                .lock()
                                .unwrap()
                                .push_str("No gamepads available.\n");
                        }

                        *is_robot_running_clone.lock().unwrap() = true;
                        let stream_clone = Arc::clone(&stream_clone);
                        let atomic_break_loop = Arc::clone(&atomic_break_loop);
                        thread::spawn(move || {
                            input_executor(Arc::clone(&stream_clone), false, atomic_break_loop)
                        });
                    }
                    event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Char('c'),
                        ..
                    }) => {
                        if !*is_robot_running_clone.lock().unwrap() {
                            terminal_string_clone.lock().unwrap().clear();
                            continue;
                        }
                        // start in teleop
                        let mut stream = stream_clone.lock().unwrap();
                        let _ = stream.write(&[4]);
                        let _ = stream.flush();
                        terminal_string_clone
                            .lock()
                            .unwrap()
                            .push_str("Shutting down run mode\n");
                        atomic_break_loop.store(true, Ordering::Release);
                        *is_robot_running_clone.lock().unwrap() = false;
                        *unripe_stream.lock().unwrap() = true;
                    }
                    event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Char('a'),
                        ..
                    }) => {
                        if *is_robot_running_clone.lock().unwrap() {
                            continue;
                        }
                        // start in teleop
                        let mut stream = stream_clone.lock().unwrap();
                        let _ = stream.write(&[3]);
                        let _ = stream.write(&[3]);
                        let _ = stream.flush();

                        terminal_string_clone
                            .lock()
                            .unwrap()
                            .push_str("Starting in autonomous mode\n");

                        *is_robot_running_clone.lock().unwrap() = true;
                    }
                    event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Up,
                        ..
                    }) => {
                        if *selected_pane.lock().unwrap() == 1 {
                            app_devices_clone.lock().unwrap().scroll_up();
                        } else if *selected_pane.lock().unwrap() == 0 {
                            app_terminal_clone.lock().unwrap().scroll_up();
                        }
                    }

                    event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Down,
                        ..
                    }) => {
                        if *selected_pane.lock().unwrap() == 1 {
                            app_devices_clone
                                .lock()
                                .unwrap()
                                .scroll_down(devices_string_clone.lock().unwrap().lines().count())
                        } else if *selected_pane.lock().unwrap() == 0 {
                            app_terminal_clone
                                .lock()
                                .unwrap()
                                .scroll_down(terminal_string_clone.lock().unwrap().lines().count())
                        }
                    }
                    event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Left,
                        ..
                    }) => {
                        *selected_pane.lock().unwrap() = 0;
                    }
                    event::Event::Key(event::KeyEvent {
                        code: event::KeyCode::Right,
                        ..
                    }) => {
                        *selected_pane.lock().unwrap() = 1;
                    }
                    _ => (),
                }
            }
        });
        let is_robot_running = Arc::clone(&is_robot_running);
        let terminal_string_clone = Arc::clone(&terminal_string);
        let mut previous_scroll_devices = 0;
        let mut previous_scroll_terminal = 0;
        let mut schedule_clear = false;

        terminal.clear().unwrap();

        loop {
            if is_robot_running.lock().as_ref().unwrap().eq(&true) {
                *devices_string.lock().unwrap() = read_devices();
            }
            if schedule_clear {
                schedule_clear = false;
                terminal.clear().unwrap();
            }
            terminal
                .draw(|frame| {
                    let horizontal = Layout::horizontal([Percentage(70), Percentage(30)]);
                    let [main_area, devices_area] = horizontal.areas(frame.area());
                    let instructions = Line::from(vec![
                        " Switch Pane ".reset(),
                        "<Left>/<Right>".blue().bold(),
                        " Autonomous ".reset(),
                        "<A>".blue().bold(),
                        " Teleop ".reset(),
                        "<S>".blue().bold(),
                        " Input ".reset(),
                        "<I>".blue().bold(),
                        " ".into(),
                    ]);

                    let lines: Vec<ListItem> = devices_string
                        .lock()
                        .unwrap()
                        .lines()
                        .map(|line| ListItem::new(line.to_string()))
                        .collect();

                    let terminal_lines: Vec<ListItem> = terminal_string_clone
                        .lock()
                        .unwrap()
                        .lines()
                        .map(|line| ListItem::new(line.to_string()))
                        .collect();
                    let devices_list = List::new(lines)
                        .block(
                            Block::bordered()
                                .title(Line::from(vec![
                                    if *is_robot_running.lock().unwrap() {
                                        " Connected ".reset().on_green().white().bold()
                                    } else {
                                        " Disconnected ".reset().on_red().white().bold()
                                    },
                                    " ".into(),
                                    "Devices".reset(),
                                    if *selected_pane_clone.lock().unwrap() == 1 {
                                        " (Selected) ".blue().bold()
                                    } else {
                                        " ".into()
                                    },
                                ]))
                                .title_bottom(
                                    Line::from(vec![
                                        " Quit ".reset(),
                                        "<Q>".blue().bold(),
                                        " Stop/Clear ".reset(),
                                        "<C>".blue().bold(),
                                        " ".into(),
                                    ])
                                    .centered(),
                                )
                                .border_style(if *selected_pane_clone.lock().unwrap() == 1 {
                                    Style::default().blue()
                                } else {
                                    Style::default()
                                }),
                        )
                        .highlight_style(Style::default().reversed());

                    let terminal_list = List::new(terminal_lines)
                        .block(
                            Block::bordered()
                                .title(Line::from(vec![
                                    " ".into(),
                                    "Terminal".reset(),
                                    if *selected_pane_clone.lock().unwrap() == 0 {
                                        " (Selected) ".blue().bold()
                                    } else {
                                        " ".into()
                                    },
                                ]))
                                .border_style(if *selected_pane_clone.lock().unwrap() == 0 {
                                    Style::default().blue()
                                } else {
                                    Style::default()
                                })
                                .title_bottom(instructions.centered()),
                        )
                        .highlight_style(Style::default().reversed());

                    if previous_scroll_terminal != app_terminal_pane.lock().unwrap().scroll {
                        schedule_clear = true;
                        previous_scroll_terminal = app_terminal_pane.lock().unwrap().scroll;
                    }
                    if previous_scroll_devices != app_devices_pane.lock().unwrap().scroll {
                        schedule_clear = true;
                        previous_scroll_devices = app_devices_pane.lock().unwrap().scroll;
                    }
                    frame.render_stateful_widget(
                        devices_list,
                        devices_area,
                        &mut ListState::default()
                            .with_offset(app_devices_pane.lock().unwrap().scroll),
                    );

                    frame.render_stateful_widget(
                        terminal_list,
                        main_area,
                        &mut ListState::default()
                            .with_offset(app_terminal_pane.lock().unwrap().scroll),
                    );
                })
                .unwrap();
        }
    }

    pub fn input_executor(
        stream: Arc<Mutex<UnixStream>>,
        utilize_stopper: bool,
        receiver: Arc<AtomicBool>,
    ) -> () {
        let stream_clone = Arc::clone(&stream);
        if utilize_stopper {
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
        }

        let mut gilrs = Gilrs::new().unwrap();

        // Iterate over all connected gamepads
        // for (_id, gamepad) in gilrs.gamepads() {
        //     println!("{} is {:?}", gamepad.name(), gamepad.power_info());
        // }

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
            if receiver.load(Ordering::Acquire) {
                // println!("STOPPED");
                break;
            }
            while let Some(Event {
                id, event, time, ..
            }) = gilrs.next_event()
            {
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
                let _ = stream.write(&[5]);
                // send the length of the message
                let input = Input {
                    connected: true,
                    buttons: bitmap,
                    axes: vec![
                        gamepad.value(Axis::LeftStickX),
                        gamepad.value(Axis::LeftStickY),
                        gamepad.value(Axis::RightStickX),
                        gamepad.value(Axis::RightStickY),
                    ],
                    source: EnumOrUnknown::new(Source::GAMEPAD),
                    special_fields: SpecialFields::default(),
                };
                let bytes = input.write_to_bytes().unwrap();
                let _ = stream.write(&[(bytes.len() & 0x00ff) as u8]);
                let _ = stream.write(&[((bytes.len() & 0xff00) >> 8) as u8]);
                let _ = stream.write(&bytes);
                let _ = stream.flush();
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}
