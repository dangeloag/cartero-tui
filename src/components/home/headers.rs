use super::{subcomponent::Subcomponent, Component, Frame, MenuItem, UserInput};
use crate::repository::local_storage::LocalStorageRepository;
use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Headers {
  repository: Arc<Mutex<LocalStorageRepository>>,
}

impl Headers {
  pub fn new(repository: Arc<Mutex<LocalStorageRepository>>) -> Self {
    Headers { repository }
  }

  pub fn draw(&self, f: &mut Frame<'_>, rect: Rect, is_focused: bool) -> Result<()> {
    let repo = self.repository.lock().unwrap();
    let headers =
      Paragraph::new(repo.get_headers()).style(Style::default().fg(Color::LightCyan)).alignment(Alignment::Left).block(
        Block::default()
          .borders(Borders::ALL)
          .style(self.get_style(is_focused))
          .title("Headers")
          .border_type(BorderType::Plain),
      );

    f.render_widget(headers, rect);

    if is_focused {
      self.set_cursor(f, rect, &repo.get_headers());
    }

    Ok(())
  }

  pub fn get_value(&self) -> String {
    let repo = self.repository.lock().unwrap();
    repo.get_headers()
  }
}

impl Subcomponent for Headers {
  fn push(&mut self, c: char) {
    let mut repo = self.repository.lock().unwrap();
    repo.push_to_headers(c);
  }

  fn pop(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.pop_headers();
  }

  fn clear(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.clear_headers();
  }
}
