pub mod daemon;
pub mod robot;
pub mod tui;
pub mod tui_readdevices;
pub mod tui_runrobot;
pub mod keymap {
  use gilrs::Button;
  use termion::event::Key;
  pub fn gamepad_mapped(button: &Button) -> u8 {
    return match button {
      Button::DPadUp => 12,
      Button::DPadDown => 13,
      Button::DPadLeft => 14,
      Button::DPadRight => 15,
      Button::Start => 9,
      // Button:: => 5,
      Button::LeftThumb => 10,
      Button::RightThumb => 11,
      Button::LeftTrigger2 => 6,
      Button::RightTrigger2 => 7,
      Button::LeftTrigger => 4,
      Button::RightTrigger => 5,
      // Button::Guide => 10,
      Button::South => 0,
      Button::East => 1,
      Button::West => 2,
      Button::North => 3,
      _ => 20, // Default case for unmapped buttons
    } 
  }
  pub fn key_map(key: &Key) -> u8 {
    match key {
      Key::Char('a') => 0,
      Key::Char('b') => 1,
      Key::Char('c') => 2,
      Key::Char('d') => 3,
      Key::Char('e') => 4,
      Key::Char('f') => 5,
      Key::Char('g') => 6,
      Key::Char('h') => 7,
      Key::Char('i') => 8,
      Key::Char('j') => 9,
      Key::Char('k') => 10,
      Key::Char('l') => 11,
      Key::Char('m') => 12,
      Key::Char('n') => 13,
      Key::Char('o') => 14,
      Key::Char('p') => 15,
      Key::Char('q') => 16,
      Key::Char('r') => 17,
      Key::Char('s') => 18,
      Key::Char('t') => 19,
      Key::Char('u') => 20,
      Key::Char('v') => 21,
      Key::Char('w') => 22,
      Key::Char('x') => 23,
      Key::Char('y') => 24,
      Key::Char('z') => 25,
      Key::Char('1') => 26,
      Key::Char('2') => 27,
      Key::Char('3') => 28,
      Key::Char('4') => 29,
      Key::Char('5') => 30,
      Key::Char('6') => 31,
      Key::Char('7') => 32,
      Key::Char('8') => 33,
      Key::Char('9') => 34,
      Key::Char('0') => 35,
      Key::Char(',') => 36,
      Key::Char('.') => 37,
      Key::Char('/') => 38,
      Key::Char(';') => 39,
      Key::Char('\'') => 40,
      Key::Char('[') => 41,
      Key::Char(']') => 42,
      Key::Left => 43,
      Key::Right => 44,
      Key::Up => 45,
      Key::Down => 46,
      _ => 255, // Default case for unmapped keys
    }
  }
}
