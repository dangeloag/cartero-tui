use crate::{
  components::home::UserInput,
  repository::local_storage::{LocalStorageRepository, RequestInput},
};
use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{prelude::*, widgets::*};
use std::sync::{Arc, Mutex};

use super::{subcomponent::Subcomponent, Component, Frame, MenuItem};

#[derive(Default)]
pub struct RequestList {
  repository: Arc<Mutex<LocalStorageRepository>>,
  req_list_state: ListState,
}

impl RequestList {
  pub fn new(repository: Arc<Mutex<LocalStorageRepository>>) -> Self {
    let mut req_list_state = ListState::default();
    req_list_state.select(None);
    RequestList { repository, req_list_state }
  }

  pub fn draw(&mut self, f: &mut Frame<'_>, rect: Rect, is_focused: bool) -> Result<()> {
    let style = self.get_style(is_focused);
    let repo = self.repository.lock().unwrap();
    let requests = repo.get_request_list();

    // Ensure selected index is within bounds
    if requests.is_empty() {
      self.req_list_state.select(None); // Nothing to highlight
    } else {
      //let selected = self.req_list_state.selected().unwrap_or(0);
      let selected = repo.get_active_request_idx();
      self.req_list_state.select(Some(selected.min(requests.len() - 1))); // Keep in bounds
    }

    let requests_list = render_reqs(requests, style);
    f.render_stateful_widget(requests_list, rect, &mut self.req_list_state);

    Ok(())
  }

  fn next(&mut self) {
    self.repository.lock().unwrap().next_request();
  }

  fn previous(&mut self) {
    self.repository.lock().unwrap().previous_request();
  }

  fn add_request(&mut self) {
    self.repository.lock().unwrap().add_request();
  }

  fn duplicate_request(&mut self) {
    self.repository.lock().unwrap().duplicate_request();
  }

  fn delete_request(&mut self) {
    self.repository.lock().unwrap().delete_request();
  }
}

fn render_reqs<'a>(user_reqs: &Vec<RequestInput>, style: Style) -> List<'a> {
  let requests = Block::default()
    .borders(Borders::ALL)
    .style(Style::default().fg(Color::LightCyan))
    .style(style)
    .title("Requests")
    .border_type(BorderType::Plain);

  let items: Vec<_> = user_reqs
    .iter()
    .map(|req| {
      ListItem::new(Line::from(vec![
        Span::styled(req.method.to_string(), req.method.get_style()),
        Span::styled(" ", Style::default()),
        Span::styled(req.path.clone(), Style::default()),
      ]))
    })
    .collect();

  let list =
    List::new(items).block(requests).highlight_style(Style::default()
             .bg(Color::Yellow)
            //.fg(Color::Yellow)
            .add_modifier(Modifier::BOLD));

  list
}

impl Subcomponent for RequestList {
  fn handle_normal_key_events(&mut self, key: KeyEvent) {
    self.handle_key_events(key);
  }

  fn handle_key_events(&mut self, key: crossterm::event::KeyEvent) {
    match key {
      KeyEvent { modifiers: _, code: KeyCode::Char('j'), kind: _, state: _ } => self.next(),
      KeyEvent { modifiers: _, code: KeyCode::Char('k'), kind: _, state: _ } => self.previous(),
      KeyEvent { modifiers: _, code: KeyCode::Char('a'), kind: _, state: _ } => self.add_request(),
      KeyEvent { modifiers: _, code: KeyCode::Char('c'), kind: _, state: _ } => self.duplicate_request(),
      KeyEvent { modifiers: _, code: KeyCode::Char('d'), kind: _, state: _ } => self.delete_request(),
      _ => {},
    }
  }

  fn push(&mut self, c: char) {}

  fn pop(&mut self) {}

  fn clear(&mut self) {}
}
