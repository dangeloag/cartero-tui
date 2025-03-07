use super::{Component, Frame, MenuItem, UserInput};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{prelude::*, widgets::*};

pub trait Subcomponent {
  fn push(&mut self, c: char);
  fn pop(&mut self);
  fn clear(&mut self);
  fn set_cursor(&self, f: &mut Frame<'_>, rect: Rect, input: &str) {
    let (x_offset, y_offset) = super::parse_coord(input); // Assuming parse_coord is available
    f.set_cursor(rect.x + x_offset, rect.y + y_offset);
  }

  fn handle_key_events(&mut self, key: KeyEvent) {
    self.handle_default_key_events(key);
  }

  fn handle_default_key_events(&mut self, key: KeyEvent) {
    match key {
      KeyEvent { modifiers: KeyModifiers::CONTROL, code: KeyCode::Char('u'), kind: _, state: _ } => self.clear(),
      KeyEvent { modifiers: _, code: KeyCode::Char(c), kind: _, state: _ } => self.push(c),
      KeyEvent { modifiers: _, code: KeyCode::Backspace, kind: _, state: _ } => self.pop(),
      _ => {},
    }
  }

  fn handle_normal_key_events(&mut self, key: KeyEvent) {}

  fn get_style(&self, is_focused: bool) -> Style {
    match is_focused {
      true => Style::default().fg(Color::Rgb(51, 255, 207)),
      false => Style::default().fg(Color::White),
    }
  }
}
