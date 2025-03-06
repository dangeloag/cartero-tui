use crate::components::home::UserInput;
use ratatui::{prelude::*, widgets::*};

use super::{subcomponent::Subcomponent, Component, Frame, MenuItem};
use color_eyre::eyre::Result;

#[derive(Default)]
pub struct Path {
  value: String,
}

impl Path {
  pub fn new() -> Self {
    Path { value: String::from("") }
  }

  pub fn draw(&self, f: &mut Frame<'_>, path_rect: Rect, is_focused: bool) -> Result<()> {
    let path = Paragraph::new(AsRef::<str>::as_ref(&self.value))
      .style(Style::default().fg(Color::LightCyan))
      .alignment(Alignment::Left)
      .block(
        Block::default()
          .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
          .style(self.get_style(is_focused))
          .title("path")
          .border_type(BorderType::Plain),
      );
    f.render_widget(path, path_rect);

    if is_focused {
      self.set_cursor(f, path_rect, &self.value);
    }

    Ok(())
  }
}

impl Subcomponent for Path {
  fn get_value(&self) -> Option<&String> {
    Some(&self.value)
  }

  fn get_value_mut(&mut self) -> Option<&mut String> {
    Some(&mut self.value)
  }

  fn set_cursor(&self, f: &mut Frame<'_>, rect: Rect, input: &str) {
    let (x_offset, y_offset) = super::parse_coord(input);
    f.set_cursor(rect.x + x_offset as u16 - 1, rect.y + y_offset as u16);
  }
}
