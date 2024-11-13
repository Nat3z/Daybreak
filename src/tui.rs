pub mod tui {
    use crossterm::{event::{self, Event, KeyEvent}, style::Stylize};
    use ratatui::{layout::{Alignment, Constraint, Layout}, style::{Style}, text::{Text, ToLine, ToText}, widgets::{Block, Paragraph, Wrap}, Frame};
    use Constraint::{Fill, Length, Min, Percentage};
    pub struct App {
        pub scroll: usize,
    }

    impl App {
        pub fn new() -> App {
            App {
                scroll: 0
            }
        }

        pub fn scroll_up(&mut self) {
            self.scroll = self.scroll.saturating_sub(1);
        }

        pub fn scroll_down(&mut self, max_scroll: usize) {
            if self.scroll < max_scroll {
                self.scroll += 1;
            }
        }
    }
}
