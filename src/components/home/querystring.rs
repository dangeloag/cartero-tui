use super::{subcomponent::Subcomponent, Component, Frame, MenuItem};
use crate::components::home::UserInput;
use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*}; // Assuming UserInput is in crate root

#[derive(Default)]
pub struct Query {
  value: String,
}

impl Query {
  pub fn new() -> Self {
    Query { value: String::from("") }
  }

  pub fn draw(&self, f: &mut Frame<'_>, rect: Rect, is_focused: bool) -> Result<()> {
    let query = Paragraph::new(AsRef::<str>::as_ref(&self.value))
      .style(Style::default().fg(Color::LightCyan))
      .alignment(Alignment::Left)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .style(self.get_style(is_focused))
          .title("Query")
          .border_type(BorderType::Plain),
      );

    f.render_widget(query, rect);

    if is_focused {
      self.set_cursor(f, rect, &self.value);
    }

    Ok(())
  }
}

impl Subcomponent for Query {
  fn get_value_mut(&mut self) -> &mut String {
    &mut self.value
  }
}
