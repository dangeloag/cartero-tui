use crate::components::home::UserInput;
use ratatui::{prelude::*, widgets::*};

use super::{subcomponent::Subcomponent, Component, Frame, MenuItem};
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct Server {
  value: String,
  method: HttpMethod,
}

impl Server {
  pub fn new(server: String) -> Self {
    Server { value: server, method: HttpMethod::GET }
  }

  pub fn draw(&self, f: &mut Frame<'_>, method_rec: Rect, server_rect: Rect, is_focused: bool) -> Result<()> {
    let method =
      Paragraph::new(self.method.to_string()).style(self.method.get_style()).alignment(Alignment::Center).block(
        Block::default()
          .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
          .style(self.get_style(is_focused))
          .title("---Method--")
          .border_type(BorderType::Plain),
      );
    f.render_widget(method, method_rec);

    let base_url = Paragraph::new(AsRef::<str>::as_ref(&self.value))
      .style(Style::default().fg(Color::LightCyan))
      .alignment(Alignment::Left)
      .block(
        Block::default()
          .borders(Borders::TOP | Borders::BOTTOM)
          .style(self.get_style(is_focused))
          .title("server")
          .border_type(BorderType::Plain),
      );
    f.render_widget(base_url, server_rect);

    if is_focused {
      self.set_cursor(f, server_rect, &self.value);
    }

    Ok(())
  }
}

impl Server {
  pub fn get_method(&self) -> HttpMethod {
    self.method
  }
}

impl Subcomponent for Server {
  fn get_value_mut(&mut self) -> Option<&mut String> {
    Some(&mut self.value)
  }

  fn set_cursor(&self, f: &mut Frame<'_>, rect: Rect, input: &str) {
    let (x_offset, y_offset) = super::parse_coord(input);
    f.set_cursor(rect.x + x_offset as u16 - 1, rect.y + y_offset as u16);
  }

  fn get_value(&self) -> Option<&String> {
    Some(&self.value)
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

  fn previous(&self) -> Self {
    match self {
      Self::GET => Self::DELETE,
      Self::POST => Self::GET,
      Self::PUT => Self::POST,
      Self::DELETE => Self::PUT,
    }
  }

  fn next(&self) -> Self {
    match self {
      Self::GET => Self::POST,
      Self::POST => Self::PUT,
      Self::PUT => Self::DELETE,
      Self::DELETE => Self::GET,
    }
  }
}
