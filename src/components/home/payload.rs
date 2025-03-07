use super::{subcomponent::Subcomponent, Component, Frame, MenuItem};
use crate::{
  components::home::UserInput,
  repository::local_storage::{self, LocalStorageRepository},
};
use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*}; // Assuming UserInput is in crate root
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Payload {
  repository: Arc<Mutex<LocalStorageRepository>>,
}

impl Payload {
  pub fn new(repository: Arc<Mutex<LocalStorageRepository>>) -> Self {
    Payload { repository }
  }

  pub fn get_value(&self) -> String {
    self.repository.lock().unwrap().get_payload()
  }

  pub fn draw(&self, f: &mut Frame<'_>, rect: Rect, is_focused: bool) -> Result<()> {
    let repo = self.repository.lock().unwrap();
    let payload =
      Paragraph::new(repo.get_payload()).style(Style::default().fg(Color::LightCyan)).alignment(Alignment::Left).block(
        Block::default()
          .borders(Borders::ALL)
          .style(self.get_style(is_focused))
          .title("Payload")
          .border_type(BorderType::Plain),
      );

    f.render_widget(payload, rect);

    if is_focused {
      self.set_cursor(f, rect, &repo.get_payload());
    }

    Ok(())
  }
}

impl Subcomponent for Payload {
  fn push(&mut self, c: char) {
    let mut repo = self.repository.lock().unwrap();
    repo.push_to_payload(c);
  }

  fn pop(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.pop_payload();
  }

  fn clear(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.clear_payload();
  }
}
