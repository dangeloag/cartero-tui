use crate::components::home::UserInput;
use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*};

use super::{subcomponent::Subcomponent, CRequest, Component, Frame, MenuItem};

#[derive(Default)]
pub struct RequestList;

impl RequestList {
  pub fn new() -> Self {
    Self
  }

  pub fn draw(
    &self,
    f: &mut Frame<'_>,
    rect: Rect,
    user_input: &UserInput,
    req_list_state: &mut ListState,
    user_requests: &Vec<CRequest>,
    is_focused: bool,
  ) -> Result<()> {
    let style = self.get_style(is_focused);
    let requests_list = render_reqs(user_requests, user_input, style);
    f.render_stateful_widget(requests_list, rect, req_list_state);

    Ok(())
  }
}

fn render_reqs<'a>(user_reqs: &Vec<CRequest>, user_input: &UserInput, style: Style) -> List<'a> {
  let requests = Block::default()
    .borders(Borders::ALL)
    .style(Style::default().fg(Color::LightCyan))
    .style(style)
    .title("Requests")
    .border_type(BorderType::Plain);

  // let url_socket = Regex::new(r"^https?://[^/]*").unwrap();
  // let url_socket = Regex::new(r"^https?://.*?/").unwrap();
  let items: Vec<_> = user_reqs
    .iter()
    .map(|req| {
      // let parsed_owned_req_path = url_socket.replace(&req.path.clone(),"").clone().to_string();
      ListItem::new(Line::from(vec![
        Span::styled(req.method.to_string(), req.method.get_style()),
        Span::styled(" ", Style::default()),
        Span::styled(req.path.clone(), Style::default()),
        // Span::styled(parsed_owned_req_path, Style::default(),)
        // Span::styled(req.url.clone().replace(r"http://", ""), Style::default(),)
      ]))
    })
    .collect();

  let list =
    List::new(items).block(requests).highlight_style(Style::default()
            // .bg(Color::Yellow)
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD));

  list
}

impl Subcomponent for RequestList {
  fn get_value_mut(&mut self) -> Option<&mut String> {
    None
  }

  fn get_value(&self) -> Option<&String> {
    None
  }
}
