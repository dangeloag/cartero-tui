use super::{subcomponent::Subcomponent, Component, Frame, MenuItem};
use crate::components::home::UserInput;
use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*}; // Assuming UserInput is in crate root

#[derive(Default)]
pub struct Payload {
  value: String,
}

impl Payload {
  pub fn new() -> Self {
    Payload { value: String::from("") }
  }

  pub fn draw(&self, f: &mut Frame<'_>, rect: Rect, is_focused: bool) -> Result<()> {
    let payload = Paragraph::new(AsRef::<str>::as_ref(&self.value))
      .style(Style::default().fg(Color::LightCyan))
      .alignment(Alignment::Left)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .style(self.get_style(is_focused))
          .title("Payload")
          .border_type(BorderType::Plain),
      );

    f.render_widget(payload, rect);

    if is_focused {
      self.set_cursor(f, rect, &self.value);
    }

    Ok(())
  }
}

impl Subcomponent for Payload {
  fn get_value_mut(&mut self) -> &mut String {
    &mut self.value
  }
}
