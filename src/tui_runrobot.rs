pub mod run_robot_tui {
    use std::{
        collections::HashMap,
        fs,
        io::{BufReader, Cursor, Read, Seek, Write},
        net::{TcpListener, TcpStream},
        os::unix::net::UnixStream,
        path::{Path, PathBuf},
        process::exit,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Sender},
            Arc, Mutex,
        },
        thread,
        time::Duration,
    };

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent};
    use gilrs::{Axis, Button, Event, GamepadId, Gilrs};
    use protobuf::{EnumOrUnknown, Message, SpecialFields};
    use ratatui::{
        layout::{Constraint, Layout},
        style::{Style, Stylize},
        text::Line,
        widgets::{Block, List, ListItem, ListState},
    };
    use rodio::source::Source as RodioSource;
    use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
    use signal_hook::{consts::SIGINT, iterator::Signals};
    use Constraint::Percentage;

    use crate::{
        keymap::gamepad_mapped,
        keymap::key_map,
        robot::robotmanager::input::{Input, Source as InputSource},
        sfx_manager::SfxManager,
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
                        let terminal_string_clone = Arc::clone(&terminal_string_clone);
                        thread::spawn(move || {
                            input_executor(
                                Arc::clone(&stream_clone),
                                false,
                                atomic_break_loop,
                                Arc::clone(&terminal_string_clone),
                            )
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
                        let terminal_string_clone = Arc::clone(&terminal_string_clone);
                        thread::spawn(move || {
                            input_executor(
                                Arc::clone(&stream_clone),
                                false,
                                atomic_break_loop,
                                Arc::clone(&terminal_string_clone),
                            )
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
        terminal_string: Arc<Mutex<String>>,
    ) -> () {
        let stream_clone = Arc::clone(&stream);

        // Create a channel for sound effect commands
        let (sfx_tx, sfx_rx) = mpsc::channel();
        let sfx_tx_clone = sfx_tx.clone();

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

                    // Send stop command through channel and wait for it to complete
                    if let Ok(_) = sfx_tx_clone.send(("stop".to_string(), true, false)) {
                        // Give time for stop sound to play
                        thread::sleep(Duration::from_millis(1000));
                        // Then stop all sounds
                        let _ = sfx_tx_clone.send(("".to_string(), false, true));
                    }
                    exit(0);
                }
            });
        }

        let mut gilrs = Gilrs::new().unwrap();
        let mut sfx_manager = match SfxManager::new() {
            Ok(manager) => {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str("Initialized sound system\n");
                // Play startup sound
                if let Err(e) = manager.play_sfx("startup", true) {
                    terminal_string
                        .lock()
                        .unwrap()
                        .push_str(&format!("Failed to play startup sound: {}\n", e));
                }
                // Start idle sound after startup
                thread::sleep(Duration::from_millis(1000)); // Wait for startup sound
                if let Err(e) = manager.play_sfx("idle", false) {
                    terminal_string
                        .lock()
                        .unwrap()
                        .push_str(&format!("Failed to play idle sound: {}\n", e));
                }
                Some(manager)
            }
            Err(e) => {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str(&format!("Failed to initialize sound system: {}\n", e));
                None
            }
        };

        let mut active_gamepad: Option<GamepadId> = None;
        let mut button_map: HashMap<Button, bool> = HashMap::new();
        let mut button_mapping: HashMap<Button, Button> = HashMap::new();
        let mut prev_stick_states = HashMap::new();

        // Initialize the axes vector
        let mut axes = [0.0f32; 4];

        let standardized_button_indices = HashMap::from([
            (Button::South, 0),
            (Button::East, 1),
            (Button::West, 2),
            (Button::North, 3),
            (Button::LeftTrigger, 4),
            (Button::RightTrigger, 5),
            (Button::LeftTrigger2, 6),
            (Button::RightTrigger2, 7),
            // (Button::LeftThumb, 8),
            // (Button::RightThumb, 9),
            (Button::Select, 10),
            (Button::Start, 11),
            (Button::DPadUp, 12),
            (Button::DPadDown, 13),
            (Button::DPadLeft, 14),
            (Button::DPadRight, 15),
        ]);

        // Try to load existing mapping first
        let config_dir = std::path::PathBuf::from(".").join(".daybreak");
        let mapping_path = config_dir.join("controller_mapping.txt");

        if let Ok(mapping_str) = fs::read_to_string(&mapping_path) {
            terminal_string
                .lock()
                .unwrap()
                .push_str("Found existing controller mapping, attempting to load...\n");
            for mapping in mapping_str.split(',') {
                if let Some((idx_str, button_str)) = mapping.split_once(':') {
                    if let Ok(idx) = idx_str.trim().parse::<i32>() {
                        // Parse button string by removing quotes and Debug formatting
                        let clean_button_str = button_str.trim().trim_matches('"');
                        let button = match clean_button_str {
                            "South" => Some(Button::South),
                            "East" => Some(Button::East),
                            "West" => Some(Button::West),
                            "North" => Some(Button::North),
                            "LeftTrigger" => Some(Button::LeftTrigger),
                            "RightTrigger" => Some(Button::RightTrigger),
                            "LeftTrigger2" => Some(Button::LeftTrigger2),
                            "RightTrigger2" => Some(Button::RightTrigger2),
                            "Select" => Some(Button::Select),
                            "Start" => Some(Button::Start),
                            "DPadUp" => Some(Button::DPadUp),
                            "DPadDown" => Some(Button::DPadDown),
                            "DPadLeft" => Some(Button::DPadLeft),
                            "DPadRight" => Some(Button::DPadRight),
                            _ => None,
                        };

                        if let Some(button) = button {
                            if let Some((&std_button, _)) = standardized_button_indices
                                .iter()
                                .find(|(_, &val)| val == idx)
                            {
                                button_mapping.insert(std_button, button);
                            }
                        }
                    }
                }
            }

            if !button_mapping.is_empty() {
                terminal_string.lock().unwrap().push_str(&format!(
                    "Successfully loaded {}/{} button mappings!\n",
                    button_mapping.len(),
                    standardized_button_indices.len()
                ));
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str(&format!("Loaded mapping: {:?}\n", button_mapping));
            } else {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str("Failed to load any valid mappings, starting interactive setup...\n");
            }

            let missing = standardized_button_indices
                .keys()
                .filter(|k| !button_mapping.contains_key(k))
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str(&format!("Missing mappings: {:?}\n", missing));
            }
        }

        // Only do interactive mapping if we don't have a complete mapping
        if button_mapping.len() != standardized_button_indices.len() {
            terminal_string
                .lock()
                .unwrap()
                .push_str("\nController button mapping setup:\n");
            terminal_string
                .lock()
                .unwrap()
                .push_str("Press each button when prompted to configure the mapping\n");
            terminal_string
                .lock()
                .unwrap()
                .push_str("Press Ctrl+C to cancel at any time\n\n");

            let button_names = vec![
                ("A/South button", Button::South),
                ("B/East button", Button::East),
                ("X/West button", Button::West),
                ("Y/North button", Button::North),
                ("Left bumper", Button::LeftTrigger),
                ("Right bumper", Button::RightTrigger),
                ("Left trigger", Button::LeftTrigger2),
                ("Right trigger", Button::RightTrigger2),
                ("Select/Back", Button::Select),
                ("Start", Button::Start),
                ("D-pad Up", Button::DPadUp),
                ("D-pad Down", Button::DPadDown),
                ("D-pad Left", Button::DPadLeft),
                ("D-pad Right", Button::DPadRight),
            ];

            // Wait for each button press
            for (index, btn) in button_names.iter().enumerate() {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str(&format!("Press the {} button...\n", btn.0));

                'button_wait: loop {
                    while let Some(Event { id, event, .. }) = gilrs.next_event() {
                        if let gilrs::EventType::ButtonPressed(button, _) = event {
                            terminal_string
                                .lock()
                                .unwrap()
                                .push_str(&format!("Mapped {} to {:?}\n", btn.0, button));
                            button_mapping.insert(btn.1, button);
                            break 'button_wait;
                        }
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }

            terminal_string
                .lock()
                .unwrap()
                .push_str("\nButton mapping complete!\n");

            // Save mapping to file
            let mapping_str = button_mapping
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{:?}:{:?}",
                        standardized_button_indices.get(k).unwrap_or(&0),
                        v
                    )
                })
                .collect::<Vec<String>>()
                .join(",");

            let config_dir = std::path::PathBuf::from(".").join(".daybreak");

            fs::create_dir_all(&config_dir).unwrap_or_default();
            if let Err(e) = fs::write(config_dir.join("controller_mapping.txt"), mapping_str) {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str(&format!("Failed to save mapping: {}\n", e));
            }
        }

        // Initialize joystick multipliers with defaults
        let mut joystick_multipliers = [1.0f32; 4]; // [LeftX, LeftY, RightX, RightY]

        // Try to load existing calibration
        let config_dir = std::path::PathBuf::from(".").join(".daybreak");
        let calibration_path = config_dir.join("joystick_calibration.txt");

        let mut should_calibrate = true;
        if let Ok(calibration_str) = fs::read_to_string(&calibration_path) {
            terminal_string
                .lock()
                .unwrap()
                .push_str("Found existing joystick calibration, attempting to load...\n");

            let values: Vec<f32> = calibration_str
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect();

            if values.len() == 4 {
                joystick_multipliers.copy_from_slice(&values);
                terminal_string.lock().unwrap().push_str(&format!(
                    "Successfully loaded calibration: Left(X:{}, Y:{}), Right(X:{}, Y:{})\n",
                    joystick_multipliers[0],
                    joystick_multipliers[1],
                    joystick_multipliers[2],
                    joystick_multipliers[3]
                ));

                should_calibrate = false;
            } else {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str("Invalid calibration file format, starting fresh calibration...\n");
            }
        }

        if should_calibrate {
            // Joystick calibration
            terminal_string
                .lock()
                .unwrap()
                .push_str("\nJoystick Calibration Setup:\n");
            terminal_string
                .lock()
                .unwrap()
                .push_str("This will calibrate the direction of each joystick axis.\n\n");

            let calibration_steps = [
                ("Push the LEFT stick FORWARD", (Axis::LeftStickY, 1)),
                ("Push the LEFT stick RIGHT", (Axis::LeftStickX, 0)),
                ("Push the RIGHT stick FORWARD", (Axis::RightStickY, 3)),
                ("Push the RIGHT stick RIGHT", (Axis::RightStickX, 2)),
            ];

            for (instruction, (axis, index)) in calibration_steps {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str(&format!("{} and hold...\n", instruction));

                let mut max_value = 0.0f32;
                let start_time = std::time::Instant::now();

                while start_time.elapsed() < Duration::from_secs(2) {
                    while let Some(Event { id, event, .. }) = gilrs.next_event() {
                        active_gamepad = Some(id);
                    }

                    if let Some(gamepad) = active_gamepad.map(|id| gilrs.gamepad(id)) {
                        let value = gamepad.value(axis);
                        if value.abs() > max_value.abs() {
                            max_value = value;
                        }
                    }
                    thread::sleep(Duration::from_millis(50));
                }

                // If the max value is negative when we expect positive, flip the multiplier
                if max_value.abs() > 0.5 {
                    // Only calibrate if we got a significant input
                    joystick_multipliers[index] = if max_value > 0.0 { 1.0 } else { -1.0 };
                    terminal_string.lock().unwrap().push_str(&format!(
                        "Calibrated! Multiplier set to {}\n",
                        joystick_multipliers[index]
                    ));
                } else {
                    terminal_string
                        .lock()
                        .unwrap()
                        .push_str("No significant input detected, keeping default multiplier\n");
                }
            }

            terminal_string
                .lock()
                .unwrap()
                .push_str("\nJoystick calibration complete!\n");

            // Save calibration to file
            let calibration_str = joystick_multipliers
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<String>>()
                .join(",");

            let config_dir = std::path::PathBuf::from(".").join(".daybreak");
            if let Err(e) = fs::write(config_dir.join("joystick_calibration.txt"), calibration_str)
            {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str(&format!("Failed to save joystick calibration: {}\n", e));
            }
        }

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
        // button_map.insert(Button::LeftThumb, false);
        // button_map.insert(Button::RightThumb, false);

        let stream_clone = Arc::clone(&stream);
        let terminal_string = Arc::new(Mutex::new(String::new()));
        let terminal_string_clone = Arc::clone(&terminal_string);

        // Spawn a thread to watch terminal string changes
        thread::spawn(move || {
            let mut last_len = 0;
            loop {
                let current = terminal_string_clone.lock().unwrap();
                if current.len() > last_len {
                    last_len = current.len();
                }
                drop(current);
                thread::sleep(Duration::from_millis(50));
            }
        });

        // open a socket to listen for external programs to send commands to the robot
        let listener = TcpListener::bind("127.0.0.1:8080").unwrap();
        listener.set_nonblocking(true);

        loop {
            // Handle any pending sound effect commands
            while let Ok((name, once, stop_all)) = sfx_rx.try_recv() {
                if let Some(ref mut sfx) = sfx_manager {
                    if stop_all {
                        if let Err(e) = sfx.stop_all() {
                            terminal_string
                                .lock()
                                .unwrap()
                                .push_str(&format!("Failed to stop all sounds: {}\n", e));
                        }
                    } else if !name.is_empty() {
                        if once {
                            if let Err(e) = sfx.play_sfx(&name, true) {
                                terminal_string
                                    .lock()
                                    .unwrap()
                                    .push_str(&format!("Failed to play {}: {}\n", name, e));
                            }
                        } else {
                            if let Err(e) = sfx.play_sfx(&name, false) {
                                terminal_string
                                    .lock()
                                    .unwrap()
                                    .push_str(&format!("Failed to play {}: {}\n", name, e));
                            }
                        }
                    }
                }
            }

            if receiver.load(Ordering::Acquire) {
                if let Some(ref mut sfx) = sfx_manager {
                    // Play stop sound before stopping all sounds
                    if let Err(e) = sfx.play_sfx("stop", true) {
                        terminal_string
                            .lock()
                            .unwrap()
                            .push_str(&format!("Failed to play stop sound: {}\n", e));
                    }
                    thread::sleep(Duration::from_millis(2000));
                    if let Err(e) = sfx.stop_all() {
                        terminal_string
                            .lock()
                            .unwrap()
                            .push_str(&format!("Failed to stop all sounds: {}\n", e));
                    }
                }
                break;
            }

            while let Some(Event {
                id, event, time, ..
            }) = gilrs.next_event()
            {
                match event {
                    gilrs::EventType::ButtonPressed(button, _) => {
                        if let Some(gamepad) = active_gamepad.map(|id| gilrs.gamepad(id)) {
                            if let Some((&std_button, _)) =
                                button_mapping.iter().find(|(_, &v)| v == button)
                            {
                                button_map.insert(std_button, true);

                                // Send sound command through channel
                                let sfx_name = match std_button {
                                    Button::South => "button_south",
                                    Button::East => "button_east",
                                    Button::West => "button_west",
                                    Button::North => "button_north",
                                    Button::DPadUp => "dpad_up",
                                    Button::DPadDown => "dpad_down",
                                    Button::DPadLeft => "dpad_left",
                                    Button::DPadRight => "dpad_right",
                                    Button::LeftTrigger => "left_bumper",
                                    Button::RightTrigger => "right_bumper",
                                    Button::LeftTrigger2 => "left_trigger",
                                    Button::RightTrigger2 => "right_trigger",
                                    Button::Select => "select",
                                    Button::Start => "start",
                                    _ => "",
                                };
                                if !sfx_name.is_empty() {
                                    let _ = sfx_tx.send((sfx_name.to_string(), false, false));
                                }
                            }
                        }
                    }
                    gilrs::EventType::ButtonReleased(button, _) => {
                        if let Some(gamepad) = active_gamepad.map(|id| gilrs.gamepad(id)) {
                            if let Some((&std_button, _)) =
                                button_mapping.iter().find(|(_, &v)| v == button)
                            {
                                button_map.insert(std_button, false);
                            }
                        }
                    }
                    _ => {}
                }
                active_gamepad = Some(id);
            }

            // Update axes from gamepad
            if let Some(gamepad) = active_gamepad.map(|id| gilrs.gamepad(id)) {
                axes[0] = gamepad.value(Axis::LeftStickX);
                axes[1] = gamepad.value(Axis::LeftStickY);
                axes[2] = gamepad.value(Axis::RightStickX);
                axes[3] = gamepad.value(Axis::RightStickY);
            }

            // Handle stick movements
            if let Some(gamepad) = active_gamepad.map(|id| gilrs.gamepad(id)) {
                let threshold = 0.5;
                let left_up = axes[1] * joystick_multipliers[1] > threshold;
                let left_down = axes[1] * joystick_multipliers[1] < -threshold;
                let left_left = axes[0] * joystick_multipliers[0] < -threshold;
                let left_right = axes[0] * joystick_multipliers[0] > threshold;

                let right_up = axes[3] * joystick_multipliers[3] > threshold;
                let right_down = axes[3] * joystick_multipliers[3] < -threshold;
                let right_left = axes[2] * joystick_multipliers[2] < -threshold;
                let right_right = axes[2] * joystick_multipliers[2] > threshold;

                // Update stick states and send sound commands
                let stick_states = vec![
                    ("stick_left_up", left_up),
                    ("stick_left_down", left_down),
                    ("stick_left_left", left_left),
                    ("stick_left_right", left_right),
                    ("stick_right_up", right_up),
                    ("stick_right_down", right_down),
                    ("stick_right_left", right_left),
                    ("stick_right_right", right_right),
                ];

                if let Some(ref sfx) = sfx_manager {
                    for (name, current_state) in stick_states {
                        let prev_state = prev_stick_states.get(name).copied().unwrap_or(false);
                        if current_state && !prev_state {
                            // Start playing the sound when stick moves to position
                            if let Err(e) = sfx.play_sfx(name, false) {
                                terminal_string
                                    .lock()
                                    .unwrap()
                                    .push_str(&format!("Failed to play {}: {}\n", name, e));
                            }
                        } else if !current_state && prev_state {
                            // Stop the sound when stick leaves position
                            if let Err(e) = sfx.stop_sfx(name) {
                                terminal_string
                                    .lock()
                                    .unwrap()
                                    .push_str(&format!("Failed to stop {}: {}\n", name, e));
                            }
                        }
                        prev_stick_states.insert(name, current_state);
                    }
                }
            }

            // always look for external connections to send commands to the robot as a gamepad
            let mut external_axes = vec![0.0, 0.0, 0.0, 0.0];
            if let Ok((mut stream, _)) = listener.accept() {
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str("Received external connection\n");
                let mut buffer = [0; 1];
                let _ = stream.read(&mut buffer);
                let command = buffer[0];
                match command {
                    // 2 means an input write
                    2 => {
                        let mut buffer = [0; 1];
                        let _ = stream.read(&mut buffer);
                        // match the buffer to a button
                        let button = standardized_button_indices
                            .iter()
                            .find(|(_, &val)| val == buffer[0] as i32)
                            .map(|(k, _)| k);
                        if let Some(button) = button {
                            button_map.insert(button.clone(), true);
                        }
                    }
                    // 3 means a button release
                    3 => {
                        let mut buffer = [0; 1];
                        let _ = stream.read(&mut buffer);
                        let button = standardized_button_indices
                            .iter()
                            .find(|(_, &val)| val == buffer[0] as i32)
                            .map(|(k, _)| k);
                        if let Some(button) = button {
                            button_map.insert(button.clone(), false);
                        }
                    }
                    // 4 means an axes write
                    4 => {
                        let mut buffer = [0; 4];
                        let _ = stream.read(&mut buffer);
                        axes[0] = buffer[0] as f32 / 127.0;
                        axes[1] = buffer[1] as f32 / 127.0;
                        axes[2] = buffer[2] as f32 / 127.0;
                        axes[3] = buffer[3] as f32 / 127.0;
                    }
                    _ => {
                        // drop the connection
                    }
                }
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
                terminal_string
                    .lock()
                    .unwrap()
                    .push_str("External connection closed\n");
            }

            let mut bitmap: u64 = 0;
            // Set bitmap based on current button_map state
            for (button, is_pressed) in button_map.iter() {
                if *is_pressed {
                    let mapped_index = gamepad_mapped(&button);
                    terminal_string
                        .lock()
                        .unwrap()
                        .push_str(&format!("{:?} is pressed\n", button));
                    bitmap |= 1 << mapped_index;
                }
            }

            let mut stream = stream.lock().unwrap();

            let _ = stream.write(&[5]);
            let input = Input {
                connected: true,
                buttons: bitmap,
                axes: axes.to_vec(), // Convert array to Vec here
                source: EnumOrUnknown::new(InputSource::GAMEPAD),
                special_fields: SpecialFields::default(),
            };
            let bytes = input.write_to_bytes().unwrap();
            let _ = stream.write(&[(bytes.len() & 0x00ff) as u8]);
            let _ = stream.write(&[((bytes.len() & 0xff00) >> 8) as u8]);
            let _ = stream.write(&bytes);
            let _ = stream.flush();
        }
    }
}
