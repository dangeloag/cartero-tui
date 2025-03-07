use super::{subcomponent::Subcomponent, Component, Frame, MenuItem};
use crate::components::home::UserInput;
use crate::repository::local_storage::{self, LocalStorageRepository};
use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*}; // Assuming UserInput is in crate root
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Query {
  repository: Arc<Mutex<LocalStorageRepository>>,
}

impl Query {
  pub fn new(repository: Arc<Mutex<LocalStorageRepository>>) -> Self {
    Query { repository }
  }

  pub fn get_value(&self) -> String {
    self.repository.lock().unwrap().get_query()
  }

  pub fn draw(&self, f: &mut Frame<'_>, rect: Rect, is_focused: bool) -> Result<()> {
    let repo = self.repository.lock().unwrap();
    let query =
      Paragraph::new(repo.get_query()).style(Style::default().fg(Color::LightCyan)).alignment(Alignment::Left).block(
        Block::default()
          .borders(Borders::ALL)
          .style(self.get_style(is_focused))
          .title("Query")
          .border_type(BorderType::Plain),
      );

    f.render_widget(query, rect);

    if is_focused {
      self.set_cursor(f, rect, &repo.get_query());
    }

    Ok(())
  }
}

impl Subcomponent for Query {
  fn push(&mut self, c: char) {
    let mut repo = self.repository.lock().unwrap();
    repo.push_to_querystring(c);
  }

  fn pop(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.pop_querystring();
  }

  fn clear(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.clear_querystring();
  }
}
