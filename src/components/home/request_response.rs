use std::process::Command;

use super::{subcomponent::Subcomponent, AppOutput, CRequest, Component, Frame, MenuItem};
use crate::components::home::UserInput;
use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*}; // Assuming UserInput is in crate root

#[derive(Default)]
pub struct RequestResponse {
  value: String,
  jq_is_installed: bool,
}

impl RequestResponse {
  pub fn new() -> Self {
    RequestResponse { value: String::from(""), jq_is_installed: jq_is_installed() }
  }

  pub fn draw(
    &self,
    f: &mut Frame<'_>,
    results_rect: Rect,
    footer_rec: Rect,
    app_output: &AppOutput,
    is_focused: bool,
  ) -> Result<()> {
    let request_result_chunk = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
      .split(results_rect);

    let result_headers = Paragraph::new(AsRef::<str>::as_ref(&app_output.response_headers))
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

    let result_payload = Paragraph::new(AsRef::<str>::as_ref(&app_output.response_payload))
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

    let response_json_path = Paragraph::new(AsRef::<str>::as_ref(&self.value))
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
      self.set_cursor(f, lower_bar_chunks[1], &self.value);
    }

    Ok(())
  }
}

fn jq_is_installed() -> bool {
  match Command::new("jq").arg("--version").output() {
    Ok(output) => output.status.success(),
    Err(_) => false,
  }
}

impl Subcomponent for RequestResponse {
  fn get_value_mut(&mut self) -> Option<&mut String> {
    Some(&mut self.value)
  }

  fn get_value(&self) -> Option<&String> {
    Some(&self.value)
  }
}
