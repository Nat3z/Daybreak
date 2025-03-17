pub mod read_devices_tui {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
        process::exit,
        sync::{Arc, Mutex},
        thread,
    };

    use crate::{
        robot::robotmanager::device::{param::Val, DevData},
        tui::tui::App,
    };
    use crossterm::event;
    use protobuf::Message;
    use ratatui::{
        layout::{Constraint, Layout},
        style::{Style, Stylize},
        widgets::{Block, List, ListItem, ListState},
    };
    use Constraint::Percentage;

    pub fn tui() {
        // In your main loop:
        let app = Arc::new(Mutex::new(App::new()));
        let app_clone = Arc::clone(&app);
        let mut devices_string = Arc::new(read_devices());

        let mut terminal = ratatui::init();
        thread::spawn(move || loop {
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
                    exit(0);
                }
                event::Event::Key(event::KeyEvent {
                    code: event::KeyCode::Up,
                    ..
                }) => app_clone.lock().unwrap().scroll_up(),
                event::Event::Key(event::KeyEvent {
                    code: event::KeyCode::Down,
                    ..
                }) => app_clone
                    .lock()
                    .unwrap()
                    .scroll_down(devices_string.lines().count()),
                _ => (),
            }
        });
        loop {
            devices_string = Arc::new(read_devices());

            let lines: Vec<ListItem> = devices_string
                .lines()
                .map(|line| ListItem::new(line.to_string()))
                .collect();

            terminal
                .draw(|frame| {
                    let vertical = Layout::vertical([Percentage(100)]);
                    let [main_area] = vertical.areas(frame.area());

                    let devices_list = List::new(lines)
                        .block(Block::bordered().title("Devices"))
                        .highlight_style(Style::default().reversed());

                    frame.render_stateful_widget(
                        devices_list,
                        main_area,
                        &mut ListState::default().with_offset(app.lock().unwrap().scroll),
                    );
                })
                .unwrap();

            // Handle scrolling input
        }
    }
    pub fn read_devices() -> String {
        for _ in 1..10 {
            let stream = UnixStream::connect(format!(
                "{}/daybreak.sock",
                std::env::temp_dir().into_os_string().into_string().unwrap()
            ));
            if stream.is_err() {
                return "[List Devices] Failed to connect to daemon.".to_string();
            }

            let mut stream = stream.unwrap();
            stream.write(&[4]).unwrap();
            stream.flush().unwrap();
            let mut buffer = [0; 3];
            stream.read(&mut buffer).unwrap();
            if buffer[0] == 0 {
                // println!("[List Devices] No robot available.");
                return "[List Devices] No robot available.".to_string();
            }

            let msg_length = (buffer[2] as usize) << 8 | buffer[1] as usize;
            let mut buffer = vec![0; msg_length];
            stream.read_exact(&mut buffer).unwrap();
            let device_data = DevData::parse_from_bytes(&buffer);
            if device_data.is_err() {
                // println!("[List Devices] Failed to parse devices list.");
                // println!("{:?}", device_data.err().unwrap());
                return "".to_string();
            }
            let device_data = device_data.unwrap();
            // TODO - Work on Parsing Device Data, make it pretty
            let devices = device_data.devices;

            if devices.len() == 0 {
                // println!("No devices available.");
                continue;
            }
            let mut built_str = String::new();
            for device in devices {
                if device.name == "CustomData" {
                    // println!("{} (Stopwatch)", device.uid);
                    built_str.push_str(&format!("{} (Stopwatch)\n", device.uid));
                } else {
                    // println!("{} ({})", device.uid, device.name);
                    built_str.push_str(&format!(
                        "{}_{} ({})\n",
                        device.type_, device.uid, device.name
                    ));
                }
                for field in device.params {
                    // turn the val into its respective data type
                    let val = field.val.as_ref().unwrap();
                    let val = match val {
                        Val::Bval(val) => val.to_string(),
                        Val::Fval(val) => val.to_string(),
                        Val::Ival(val) => val.to_string(),
                        _ => "Unknown".to_string(),
                    };
                    built_str.push_str(&format!("{} - {}\n", field.name, val));
                    // println!("{} - {}", field.name, val);
                }
                built_str.push_str("\n\n");
                // println!("\n");
            }
            return built_str;
        }
        return "".to_string();
    }
}
