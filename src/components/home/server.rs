use std::sync::{Arc, Mutex};

use crate::repository::local_storage::LocalStorageRepository;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{prelude::*, widgets::*};
use tracing::info;

use super::{
  subcomponent::{parse_coord, Subcomponent},
  Component, Frame, MenuItem,
};
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct Server {
  repository: Arc<Mutex<LocalStorageRepository>>,
}

impl Server {
  pub fn new(repo: Arc<Mutex<LocalStorageRepository>>) -> Self {
    let server = Server { repository: Arc::clone(&repo), ..Default::default() };
    server
  }

  pub fn get_value(&self) -> String {
    let repo = self.repository.lock().unwrap();
    repo.get_server()
  }

  pub fn draw(&self, f: &mut Frame<'_>, method_rec: Rect, server_rect: Rect, is_focused: bool) -> Result<()> {
    let repo = self.repository.lock().unwrap();
    let method = Paragraph::new(repo.get_method().to_string())
      .style(repo.get_method().get_style())
      .alignment(Alignment::Center)
      .block(
        Block::default()
          .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
          .style(self.get_style(is_focused))
          .title("---Method--")
          .border_type(BorderType::Plain),
      );
    f.render_widget(method, method_rec);

    let base_url =
      Paragraph::new(repo.get_server()).style(Style::default().fg(Color::LightCyan)).alignment(Alignment::Left).block(
        Block::default()
          .borders(Borders::TOP | Borders::BOTTOM)
          .style(self.get_style(is_focused))
          .title("server")
          .border_type(BorderType::Plain),
      );
    f.render_widget(base_url, server_rect);

    if is_focused {
      self.set_cursor(f, server_rect, &repo.get_server());
    }

    Ok(())
  }
}

impl Server {
  pub fn get_method(&self) -> HttpMethod {
    let repo = self.repository.lock().unwrap();
    repo.get_method()
  }
}

impl Subcomponent for Server {
  fn set_cursor(&self, f: &mut Frame<'_>, rect: Rect, input: &str) {
    let (x_offset, y_offset) = parse_coord(input);
    f.set_cursor(rect.x + x_offset as u16 - 1, rect.y + y_offset as u16);
  }

  fn handle_normal_key_events(&mut self, key: KeyEvent) {
    match key {
      KeyEvent { modifiers: _, code: KeyCode::Char('m'), kind: _, state: _ } => {
        let mut repo = self.repository.lock().unwrap();
        repo.set_next_method()
      },
      KeyEvent { modifiers: _, code: KeyCode::Char('M'), kind: _, state: _ } => {
        let mut repo = self.repository.lock().unwrap();
        repo.set_previous_method()
      },
      _ => {},
    }
  }

  fn push(&mut self, c: char) {
    let mut repo = self.repository.lock().unwrap();
    repo.push_to_server(c);
  }

  fn pop(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.pop_server();
  }

  fn clear(&mut self) {
    let mut repo = self.repository.lock().unwrap();
    repo.clear_server();
  }
}

#[derive(Default, Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum HttpMethod {
  #[default]
  GET,
  POST,
  PUT,
  DELETE,
}

impl HttpMethod {
  pub fn to_string(&self) -> String {
    match self {
      Self::GET => String::from("GET"),
      Self::POST => String::from("POST"),
      Self::PUT => String::from("PUT"),
      Self::DELETE => String::from("DELETE"),
    }
  }

  pub fn get_style(&self) -> Style {
    match self {
      Self::GET => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
      Self::POST => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
      Self::PUT => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
      Self::DELETE => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
  }

  pub fn previous(&self) -> Self {
    match self {
      Self::GET => Self::DELETE,
      Self::POST => Self::GET,
      Self::PUT => Self::POST,
      Self::DELETE => Self::PUT,
    }
  }

  pub fn next(&self) -> Self {
    match self {
      Self::GET => Self::POST,
      Self::POST => Self::PUT,
      Self::PUT => Self::DELETE,
      Self::DELETE => Self::GET,
    }
  }
}
