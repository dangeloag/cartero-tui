use super::{subcomponent::Subcomponent, CRequest, Component, Frame, MenuItem, UserInput};
use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*}; // Assuming UserInput is in crate root

#[derive(Default)]
pub struct Headers {
  value: String,
}

impl Headers {
  pub fn new() -> Self {
    Headers { value: "".to_string() }
  }

  pub fn draw(&self, f: &mut Frame<'_>, rect: Rect, is_focused: bool) -> Result<()> {
    let headers = Paragraph::new(AsRef::<str>::as_ref(&self.value))
      .style(Style::default().fg(Color::LightCyan))
      .alignment(Alignment::Left)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .style(self.get_style(is_focused))
          .title("Headers")
          .border_type(BorderType::Plain),
      );

    f.render_widget(headers, rect);

    if is_focused {
      self.set_cursor(f, rect, &self.value);
    }

    Ok(())
  }
}

impl Subcomponent for Headers {
  fn get_value_mut(&mut self) -> Option<&mut String> {
    Some(&mut self.value)
  }

  fn get_value(&self) -> Option<&String> {
    Some(&self.value)
  }
}
