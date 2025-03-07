use std::{
  io::{Read, Write},
  process::{Command, Stdio},
};

use super::{subcomponent::Subcomponent, Component, Frame, MenuItem, ReqResponse};
use crate::components::home::UserInput;
use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*};
use serde::Serialize;
use serde_json::Value;
use tracing::{debug, error, info, trace, warn};

pub struct RequestResponse {
  response_headers: String,
  response_body: String,
  response_body_last: String,
  body_filter: String,
  jq_is_installed: bool,
}

impl RequestResponse {
  pub fn new() -> Self {
    RequestResponse {
      response_headers: String::from(""),
      response_body: String::from(""),
      response_body_last: String::from(""),
      body_filter: String::from(""),
      jq_is_installed: jq_is_installed(),
    }
  }

  pub fn set_response(&mut self, req_response: ReqResponse) {
    self.response_headers = req_response.headers;

    match serde_json::from_str::<Value>(&req_response.body) {
      Ok(json) => match serde_json::to_string_pretty(&json) {
        Ok(pretty_json) => {
          self.response_body = pretty_json;
        },
        Err(e) => {
          self.response_body = format!("JSON serialization error: {}", e);
        },
      },
      Err(e) => {
        self.response_body = req_response.body.clone();
      },
    }

    self.response_body_last = req_response.body;
  }

  pub fn draw(&mut self, f: &mut Frame<'_>, results_rect: Rect, footer_rec: Rect, is_focused: bool) -> Result<()> {
    let request_result_chunk = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
      .split(results_rect);

    let result_headers = Paragraph::new(AsRef::<str>::as_ref(&self.response_headers))
      .style(Style::default().fg(Color::Green))
      .alignment(Alignment::Left)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .style(Style::default().fg(Color::White))
          .title("Response Headers")
          .border_type(BorderType::Plain),
      );
    f.render_widget(result_headers, request_result_chunk[0]);

    let result_payload = Paragraph::new(AsRef::<str>::as_ref(&self.response_body))
      .style(Style::default().fg(Color::LightCyan))
      .alignment(Alignment::Left)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .style(Style::default().fg(Color::White))
          .title("Response Payload")
          .border_type(BorderType::Plain),
      );
    f.render_widget(result_payload, request_result_chunk[1]);

    let lower_bar_chunks = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([Constraint::Length(80), Constraint::Min(50)].as_ref())
      .split(footer_rec);

    let copyright = Paragraph::new("HTTP Request Explorer")
      .style(Style::default().fg(Color::LightCyan))
      .alignment(Alignment::Center)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .style(Style::default().fg(Color::White))
          .title("Copyright")
          .border_type(BorderType::Plain),
      );
    f.render_widget(copyright, lower_bar_chunks[0]);

    let json_path_title = if self.jq_is_installed { "JQ" } else { "Serde Pointer" };

    let response_json_path = Paragraph::new(AsRef::<str>::as_ref(&self.body_filter))
      .style(Style::default().fg(Color::LightCyan))
      .alignment(Alignment::Left)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .style(self.get_style(is_focused))
          .title(json_path_title)
          .border_type(BorderType::Plain),
      );
    f.render_widget(response_json_path, lower_bar_chunks[1]);

    if is_focused {
      self.set_cursor(f, lower_bar_chunks[1], &self.body_filter);
    }

    Ok(())
  }

  fn parse_with_serde(&mut self) {
    match serde_json::from_str::<Value>(&self.response_body_last) {
      Ok(json_value) => {
        if let Some(field_value) = json_value.pointer(&self.response_body) {
          let field_str = match field_value {
            Value::Number(n) => {
              if n.is_u64() {
                n.as_u64().unwrap().to_string()
              } else if n.is_i64() {
                n.as_i64().unwrap().to_string()
              } else if n.is_f64() {
                n.as_f64().unwrap().to_string()
              } else {
                "".to_string()
              }
            },
            Value::String(s) => s.to_string(),
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            _ => field_value.to_string(),
          };
          self.response_body = field_str;
        } else {
          self.response_body = self.response_body_last.clone();
        }
      },
      _ => (),
    }
  }

  fn parse_with_jq(&mut self) {
    let mut child = Command::new("jq")
            .arg(&self.body_filter) // <-- Pass the jq filter directly as an argument
            .stdin(Stdio::piped())  // Provide input via stdin
            .stdout(Stdio::piped()) // Capture stdout
            .stderr(Stdio::null())  // Discard error output
            .spawn()
            .expect("Failed to start jq command");

    if let Some(mut stdin) = child.stdin.take() {
      stdin.write_all(self.response_body_last.as_bytes()).expect("Failed to write to jq stdin");
    }

    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
      stdout.read_to_string(&mut output).expect("Failed to read jq output");
    }

    let status = child.wait().expect("Failed to wait for jq process");

    if status.success() {
      self.response_body = output;
    } else {
      self.response_body = self.response_body_last.clone();
    }
  }
}

fn jq_is_installed() -> bool {
  debug!("Checking if jq is installed..."); // Add debug logging

  match Command::new("jq").arg("--version").output() {
    Ok(output) => {
      if output.status.success() {
        info!("jq is installed."); // Log successful installation
        true
      } else {
        error!("jq is not installed (non-zero exit code).");
        debug!("jq output: {:?}", output); // log full output for debugging
        false
      }
    },
    Err(err) => {
      error!("jq is not installed (command execution failed): {}", err);
      false
    },
  }
}

impl Subcomponent for RequestResponse {
  //fn handle_key_events(&mut self, key: crossterm::event::KeyEvent) {
  //  self.handle_default_key_events(key);
  //  if self.jq_is_installed {
  //    self.parse_with_jq();
  //  }
  //}

  fn push(&mut self, c: char) {
    self.body_filter.push(c);
    if self.jq_is_installed {
      self.parse_with_jq();
    }
  }

  fn pop(&mut self) {
    self.body_filter.pop();
  }

  fn clear(&mut self) {
    self.body_filter.clear();
  }
}

impl Default for RequestResponse {
  fn default() -> Self {
    Self::new()
  }
}
