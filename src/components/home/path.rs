use ratatui::{prelude::*, widgets::*};

use super::{
  subcomponent::{parse_coord, Subcomponent},
  Component, Frame, MenuItem,
};
use crate::repository::local_storage::{self, LocalStorageRepository};
use color_eyre::eyre::Result;
use std::sync::{Arc, Mutex}; // Assuming UserInput is in crate root

#[derive(Default)]
pub struct Path {
  repository: Arc<Mutex<LocalStorageRepository>>,
}

impl Path {
  pub fn new(repo: Arc<Mutex<LocalStorageRepository>>) -> Self {
    let path = Path { repository: repo };
    path
  }

  pub fn get_value(&self) -> String {
    self.repository.lock().unwrap().get_path()
  }

  pub fn draw(&self, f: &mut Frame<'_>, path_rect: Rect, is_focused: bool) -> Result<()> {
    let repo = self.repository.lock().unwrap();
    let path =
      Paragraph::new(repo.get_path()).style(Style::default().fg(Color::LightCyan)).alignment(Alignment::Left).block(
        Block::default()
          .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
          .style(self.get_style(is_focused))
          .title("path")
          .border_type(BorderType::Plain),
      );
    f.render_widget(path, path_rect);

    if is_focused {
      self.set_cursor(f, path_rect, &repo.get_path());
    }

    Ok(())
  }
}

impl Subcomponent for Path {
  fn set_cursor(&self, f: &mut Frame<'_>, rect: Rect, input: &str) {
    let (x_offset, y_offset) = parse_coord(input);
    f.set_cursor(rect.x + x_offset as u16 - 1, rect.y + y_offset as u16);
  }

  fn push(&mut self, c: char) {
    let mut repo = self.repository.lock().unwrap();
    repo.push_to_path(c);
  }

  fn pop(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.pop_path();
  }

  fn clear(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.clear_path();
  }
}
