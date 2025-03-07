use chrono::prelude::*;
use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use fancy_regex::Regex;
use futures::{executor::block_on, StreamExt};
use log::error;
use ratatui::{prelude::*, widgets::*};
use reqwest::{
  header::{HeaderMap, HeaderName, HeaderValue},
  Response,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
  collections::HashMap,
  str::FromStr,
  sync::{Arc, Mutex},
  thread,
  time::Duration,
};
use std::{
  fs,
  fs::File,
  io::prelude::*,
  process::Command,
  process::Stdio,
  time::{SystemTime, UNIX_EPOCH},
};
use std::{sync::mpsc, time};
use subcomponent::Subcomponent;
use tempfile::tempfile;
use tokio::{
  runtime::Runtime,
  sync::mpsc::{Sender, UnboundedReceiver, UnboundedSender},
  task::{self, spawn_blocking},
};
use tracing::trace;
use tui_input::{backend::crossterm::EventHandler, Input};

use super::{Component, Frame};
use crate::{
  action::Action,
  repository::local_storage::{self, LocalStorageRepository},
};

mod headers;
mod path;
mod payload;
mod querystring;
mod request_list;
mod request_response;
pub(crate) mod server;
mod subcomponent;

#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub enum Mode {
  #[default]
  Normal,
  Insert,
  Processing,
}

#[derive(Default, Clone)]
struct ReqResponse {
  headers: String,
  body: String,
}

#[derive(Default)]
pub struct Home {
  pub show_help: bool,
  pub counter: usize,
  pub app_ticker: usize,
  pub render_ticker: usize,
  pub mode: Mode,
  pub input: Input,
  pub action_tx: Option<UnboundedSender<Action>>,
  pub tx: Option<mpsc::Sender<ReqResponse>>,
  pub rx: Option<mpsc::Receiver<ReqResponse>>,
  pub keymap: HashMap<KeyEvent, Action>,
  pub text: Vec<String>,
  pub last_events: Vec<KeyEvent>,
  pub server_list_state: ListState,
  pub repository: Arc<Mutex<LocalStorageRepository>>,
  //pub servers: Servers,
  //pub user_req: CRequest,
  //pub user_input: UserInput,
  //pub app_output: AppOutput,
  pub popup: bool,
  pub parsing_rules_popup: bool,

  pub config: Option<crate::config::Config>,
  pub server: server::Server,
  pub path: path::Path,
  pub request_list: request_list::RequestList,
  pub querystring: querystring::Query,
  pub payload: payload::Payload,
  pub headers: headers::Headers,
  pub request_response: request_response::RequestResponse,
  pub active_widget: MenuItem,
}

impl Home {
  pub fn new(repository: Arc<Mutex<LocalStorageRepository>>) -> Self {
    let (tx, rx) = mpsc::channel();
    let server = server::Server::new(Arc::clone(&repository));
    let path = path::Path::new(Arc::clone(&repository));
    let querystring = querystring::Query::new(Arc::clone(&repository));
    let payload = payload::Payload::new(Arc::clone(&repository));
    let headers = headers::Headers::new(Arc::clone(&repository));
    let request_list = request_list::RequestList::new(Arc::clone(&repository));
    Home {
      tx: Some(tx),
      rx: Some(rx),
      repository,
      server,
      path,
      request_list,
      querystring,
      payload,
      headers,
      ..Default::default()
    }
  }

  pub fn keymap(mut self, keymap: HashMap<KeyEvent, Action>) -> Self {
    self.keymap = keymap;
    self
  }

  pub fn tick(&mut self) {
    log::info!("Tick");
    self.app_ticker = self.app_ticker.saturating_add(1);
    self.last_events.drain(..);
  }

  pub fn render_tick(&mut self) {
    log::debug!("Render Tick");
    self.render_ticker = self.render_ticker.saturating_add(1);
  }

  fn get_active_widget(&mut self) -> &mut dyn Subcomponent {
    match self.active_widget {
      MenuItem::Server => &mut self.server,
      MenuItem::Path => &mut self.path,
      MenuItem::Requests => &mut self.request_list,
      MenuItem::Query => &mut self.querystring,
      MenuItem::Payload => &mut self.payload,
      MenuItem::Headers => &mut self.headers,
      MenuItem::ServerListPopup => &mut self.server,
      MenuItem::JsonPath => &mut self.request_response,
      // MenuItem::ParsingRulesPopup => &mut self.parsing_rules,
      _ => panic!("Not implemented"),
    }
  }

  fn process_request(&mut self) {
    let response_result = self.send_request();
    match response_result {
      Ok(req_response) => {
        self.request_response.set_response(req_response);
      },
      Err(shoot) => {
        panic!("ahhhh")
      },
    }
  }

  fn send_request(&self) -> Result<ReqResponse, Box<dyn std::error::Error>> {
    let tx = self.tx.clone().unwrap();
    let query = parse_query(&self.querystring.get_value()).unwrap();
    let url = format!("{}{}?{}", &self.server.get_value(), &self.path.get_value(), query);
    let headers: HeaderMap = self.parse_headers().unwrap();
    let method = self.server.get_method().clone();
    let payload: String = self.payload.get_value();

    spawn_blocking(move || {
      let client = reqwest::blocking::Client::new();
      let req_builder: Option<reqwest::blocking::RequestBuilder>;
      match method {
        server::HttpMethod::GET => {
          req_builder = Some(client.get(url).headers(headers));
        },
        server::HttpMethod::POST => {
          req_builder = Some(client.post(url).headers(headers).body(payload));
        },
        _ => {
          panic!("Method not implemented")
        },
      }

      let response_result = req_builder.unwrap().send();
      match response_result {
        Ok(response) => {
          let headers = format!("{:?}", response.headers()).replace("\",", "\n").replace("{", " ").replace("}", "");
          match response.text() {
            Ok(text) => tx.send(ReqResponse { body: text, headers }).unwrap(),
            Err(err) => tx.send(ReqResponse { body: err.to_string(), headers }).unwrap(),
          }
        },
        Err(err) => tx.send(ReqResponse { body: err.to_string(), headers: String::from("") }).unwrap(),
      }
    });

    loop {
      let message = self.rx.as_ref().unwrap().try_recv();
      match message {
        Ok(msg) => return Ok(msg),
        Err(mpsc::TryRecvError::Empty) => thread::sleep(time::Duration::from_millis(100)),
        Err(mpsc::TryRecvError::Disconnected) => {
          panic!("Disconnected ")
        },
      };
    }
  }

  fn parse_headers(&self) -> Result<HeaderMap, Box<dyn std::error::Error>> {
    let headers = self.headers.get_value();
    if headers.len() == 0 {
      return Ok(HeaderMap::new());
    }
    // TODO: fix this
    //let headers_with_env = replace_env_variables(headers, &self.local_storage.env).clone();
    let headers_with_env = headers.clone();
    let new_headers = Regex::new("\n+$").unwrap().replace_all(&headers_with_env, "");
    // TODO: move regex compiltion out  of function
    let valid_format = Regex::new(r"^((?>[^:\n\s]+\s?:[^:\n]+)\n?)+$").unwrap().is_match(&headers).unwrap();
    if !valid_format {
      return Err(Box::from("Not valid header format"));
    }
    let new_headers: Vec<(String, String)> = headers
      .split("\n")
      .map(|h| {
        let kv: Vec<&str> = h.split(":").map(|v| v.trim()).collect();
        (kv[0].to_string(), kv[1].to_string())
      })
      .collect();
    let mut header_map = HeaderMap::new();
    for (name, value) in new_headers {
      let header_name = HeaderName::from_bytes(name.as_bytes()).map_err(|e| Box::new(e))?;
      let header_value = HeaderValue::from_str(&value)?;
      header_map.insert(header_name, header_value);
    }
    Ok(header_map)
  }

  fn focus_next_widget(&mut self) {
    self.active_widget = self.active_widget.next()
  }

  fn focus_previous_widget(&mut self) {
    self.active_widget = self.active_widget.previous()
  }
}

impl Component for Home {
  fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
    self.action_tx = Some(tx);
    Ok(())
  }

  fn register_config_handler(&mut self, config: crate::config::Config) -> Result<()> {
    self.config = Some(config);
    Ok(())
  }

  fn handle_key_events(&mut self, key: KeyEvent) -> Result<Option<Action>> {
    self.last_events.push(key.clone());

    match key {
      // match global keybindings
      KeyEvent { modifiers: KeyModifiers::CONTROL, code: KeyCode::Char('s'), kind: _, state: _ } => {
        let repo = self.repository.lock().unwrap();
        repo.save();
      },
      KeyEvent { modifiers: _, code: KeyCode::Tab, kind: _, state: _ } => self.focus_next_widget(),
      KeyEvent { modifiers: _, code: KeyCode::BackTab, kind: _, state: _ } => self.focus_previous_widget(),
      KeyEvent { modifiers: _, code: KeyCode::Enter, kind: _, state: _ } => self.process_request(),
      _ => match self.mode {
        // if no global match, match mode specific keybindings
        Mode::Normal => match key {
          KeyEvent { modifiers: _, code: KeyCode::Char('i'), kind: _, state: _ } => self.mode = Mode::Insert,
          KeyEvent { modifiers: _, code: KeyCode::Char('q'), kind: _, state: _ } => {
            // TODO: figure out config stuff
            // if let Some(config) = self.config {
            //   if let Some(keymap) = config.keybindings.get(&self.mode) {
            //     if let Some(action) = keymap.get(&vec![key.clone()]) {
            //       return Ok(Some(action.clone()));
            //     }
            //   }
            // }
            return Ok(Some(Action::Quit));
          },
          KeyEvent { modifiers: _, code: KeyCode::Char(c), kind: _, state: _ } => {
            self.get_active_widget().handle_normal_key_events(key)
          },
          _ => {},
        },
        Mode::Insert => match key {
          KeyEvent { modifiers: _, code: KeyCode::Esc, kind: _, state: _ } => self.mode = Mode::Normal,
          _ => self.get_active_widget().handle_key_events(key),
        },
        _ => {},
      },
    }

    // match key {
    //   KeyEvent { modifiers: _, code: KeyCode::Enter, kind: _, state: _ } => {
    //     if self.user_input.active_menu_item == MenuItem::Server
    //       || self.user_input.active_menu_item == MenuItem::Path
    //       || self.user_input.active_menu_item == MenuItem::Requests
    //     {
    //       self.process_request();
    //     } else {
    //       self.get_active_widget().handle_key_events(key);
    //     }
    //   },
    //   KeyEvent { modifiers: _, code: KeyCode::Char(c), kind: _, state: _ } => {
    //     // self.user_input.get_active().push(c);
    //     self.get_active_widget().handle_key_events(key);
    //   },
    //   _ => {},
    // }
    Ok(Some(Action::Update))
  }

  fn update(&mut self, action: Action) -> Result<Option<Action>> {
    match action {
      Action::Tick => self.tick(),
      Action::Render => self.render_tick(),
      Action::ToggleShowHelp if self.mode == Mode::Normal => self.show_help = !self.show_help,
      Action::EditInput => {
        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        if self.input.value().len() > 0 {
          temp_file.write_all(self.input.value().as_bytes()).unwrap();
        } else {
          temp_file.write_all("go crazy here".as_bytes()).unwrap();
        }

        let vim_cmd = format!("vim {}", temp_file.path().display());
        let mut output = std::process::Command::new("sh")
          .arg("-c")
          .arg(&vim_cmd)
          .stdin(Stdio::piped())
          .stdout(Stdio::inherit())
          .stderr(Stdio::inherit())
          .spawn()
          .expect("Can run vim cmd");
        let vim_cmd_result = output.wait().expect("Run exits ok");
        if !vim_cmd_result.success() {
          error!("Vim exited with status code {}", vim_cmd_result.code().unwrap_or(-1))
        }

        let mut edited_text = String::new();
        std::fs::File::open(temp_file.path()).unwrap().read_to_string(&mut edited_text).unwrap();

        self.input.reset();
        self.input = Input::new(edited_text);
        let tx = self.action_tx.clone().unwrap();
        tx.send(Action::FocusGained).unwrap();
      },
      Action::EnterNormal => {
        self.mode = Mode::Normal;
      },
      Action::EnterInsert => {
        self.mode = Mode::Insert;
      },
      Action::EnterProcessing => {
        self.mode = Mode::Processing;
      },
      Action::ExitProcessing => {
        // TODO: Make this go to previous mode instead
        self.mode = Mode::Normal;
      },
      _ => (),
    }
    Ok(None)
  }

  fn draw(&mut self, f: &mut Frame<'_>, rect: Rect) -> Result<()> {
    // divide the layout in:
    // header: method, server, path
    // body: request data
    // footer: copyright and request response filter
    let chunks = Layout::default()
      .direction(Direction::Vertical)
      .margin(2)
      .constraints([Constraint::Length(3), Constraint::Min(2), Constraint::Length(3)].as_ref())
      .split(rect);

    let top_bar = chunks[0];
    let body = chunks[1];
    let footer = chunks[2];

    let method_server_path_chunks = Layout::default()
      .direction(Direction::Horizontal)
      .constraints(
        [
          Constraint::Length(10 as u16 + 10),
          Constraint::Length(self.server.get_value().len() as u16 + 2),
          Constraint::Min(50),
        ]
        .as_ref(),
      )
      .split(top_bar);

    let method = method_server_path_chunks[0];
    let server = method_server_path_chunks[1];
    let path = method_server_path_chunks[2];

    let request_chunk = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([Constraint::Length(30), Constraint::Length(50), Constraint::Min(50)].as_ref())
      .split(body);

    let request_data_chunk = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Percentage(30), Constraint::Percentage(35), Constraint::Percentage(35)].as_ref())
      .split(request_chunk[1]);

    let _ = self.server.draw(f, method, server, is_focused(self.active_widget, MenuItem::Server));

    let _ = self.path.draw(f, path, is_focused(self.active_widget, MenuItem::Path));

    let _ = self.request_list.draw(f, request_chunk[0], is_focused(self.active_widget, MenuItem::Requests));

    let _ = self.querystring.draw(f, request_data_chunk[0], is_focused(self.active_widget, MenuItem::Query));

    let _ = self.payload.draw(f, request_data_chunk[1], is_focused(self.active_widget, MenuItem::Payload));

    let _ = self.headers.draw(f, request_data_chunk[2], is_focused(self.active_widget, MenuItem::Headers));

    let _ = self.request_response.draw(f, request_chunk[2], footer, is_focused(self.active_widget, MenuItem::JsonPath));

    //if self.popup {
    //  let requests_list2 = render_popup(&vec![], &self.user_input);
    //  let _ = Block::default().title("Popup").borders(Borders::ALL);
    //  let area = centered_rect(60, 20, rect);
    //  f.render_widget(Clear, area); //this clears out the background
    //  f.render_stateful_widget(requests_list2, area, &mut self.server_list_state);
    //  self.user_input.active_menu_item = MenuItem::ServerListPopup;
    //}

    //if self.parsing_rules_popup {
    //  let _ = Block::default().title("Popup").borders(Borders::ALL);
    //  let area = centered_rect(50, 30, rect);
    //  let rules_text = Paragraph::new(AsRef::<str>::as_ref(&self.user_input.parsing_rules))
    //    .style(Style::default().fg(Color::LightCyan))
    //    .alignment(Alignment::Left)
    //    .block(
    //      Block::default()
    //        .borders(Borders::ALL)
    //        .style(focused_style(&self.user_input, MenuItem::ParsingRulesPopup))
    //        .title("Parsing Rules")
    //        .border_type(BorderType::Plain),
    //    );
    //  f.render_widget(Clear, area); //this clears out the background
    //  f.render_widget(rules_text, area);
    //  self.user_input.active_menu_item = MenuItem::ParsingRulesPopup;
    //}

    Ok(())
  }
}

#[derive(Serialize, Deserialize, Clone)]
struct UserInput {
  method: server::HttpMethod,
  server: String,
  path: String,
  query: String,
  payload: String,
  json_path: String,
  headers: String,
  parsing_rules: String,
  active_menu_item: MenuItem,
}

//impl From<RequestInput> for UserInput {
//  fn from(c_req: RequestInput) -> Self {
//    UserInput {
//      server: c_req.server,
//      path: c_req.path,
//      query: c_req.query,
//      payload: c_req.payload,
//      json_path: String::new(),
//      headers: c_req.headers,
//      parsing_rules: c_req.parsing_rules,
//      method: c_req.method,
//      active_menu_item: MenuItem::Server,
//    }
//  }
//}

//#[derive(Serialize, Deserialize, Clone)]
//struct RequestInput {
//  method: server::HttpMethod,
//  server: String,
//  path: String,
//  query: String,
//  payload: String,
//  headers: String,
//  #[serde(default = "emtpy_string")]
//  parsing_rules: String,
//}

fn emtpy_string() -> String {
  String::from("")
}

fn default_env() -> HashMap<String, String> {
  HashMap::new()
}

//impl From<UserInput> for RequestInput {
//  fn from(user_input: UserInput) -> Self {
//    RequestInput {
//      server: user_input.server,
//      path: user_input.path,
//      query: user_input.query,
//      payload: user_input.payload,
//      headers: user_input.headers,
//      method: user_input.method,
//      parsing_rules: user_input.parsing_rules,
//    }
//  }
//}

#[derive(Default, Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
enum MenuItem {
  #[default]
  Server,
  Path,
  Query,
  Payload,
  Headers,
  Requests,
  JsonPath,
  ServerListPopup,
  ParsingRulesPopup,
}

impl MenuItem {
  fn next(&self) -> Self {
    let curr: usize = usize::from(*self);
    MenuItem::from(curr + 1)
  }

  fn previous(&self) -> Self {
    let curr: usize = usize::from(*self);
    if curr == 0 {
      MenuItem::JsonPath
    } else {
      MenuItem::from(curr - 1)
    }
  }
}

impl From<MenuItem> for usize {
  fn from(input: MenuItem) -> usize {
    match input {
      MenuItem::Server => 0,
      MenuItem::Path => 1,
      MenuItem::Requests => 2,
      MenuItem::Query => 3,
      MenuItem::Payload => 4,
      MenuItem::Headers => 5,
      MenuItem::JsonPath => 6,
      MenuItem::ServerListPopup => 0,
      MenuItem::ParsingRulesPopup => 0,
    }
  }
}

impl From<usize> for MenuItem {
  fn from(input: usize) -> MenuItem {
    match input {
      0 => MenuItem::Server,
      1 => MenuItem::Path,
      2 => MenuItem::Requests,
      3 => MenuItem::Query,
      4 => MenuItem::Payload,
      5 => MenuItem::Headers,
      6 => MenuItem::JsonPath,
      _ => MenuItem::Server,
    }
  }
}

fn parse_rules(rules: &str) -> Vec<(String, String)> {
  let rules_vec: Vec<&str> = rules.split('\n').collect();

  let re = Regex::new(r"([a-zA-Z_.-]+)\s*->\s*(/.*)").unwrap();
  let mut fields_vec: Vec<(String, String)> = vec![];

  for rule in rules_vec {
    if let Some(caps) = re.captures(rule).unwrap() {
      let field_name = caps.get(1).unwrap().as_str();
      let json_path = caps.get(2).unwrap().as_str();
      fields_vec.push((field_name.to_owned(), json_path.to_owned()));
    }
  }
  return fields_vec;
}

fn is_focused(active_item: MenuItem, item: MenuItem) -> bool {
  active_item == item
}

fn focused_style(user_input: &UserInput, item: MenuItem) -> Style {
  if user_input.active_menu_item == item {
    Style::default().fg(Color::Rgb(51, 255, 207))
  } else {
    Style::default().fg(Color::White)
  }
}

fn parse_coord(text: &str) -> (u16, u16) {
  let list: Vec<&str> = text.split("\n").collect();
  let x_offset = list.last().unwrap().len() as u16 + 1;
  let y_offset = list.len() as u16;
  (x_offset, y_offset)
}

fn parse_query(query: &str) -> Result<String, Box<dyn std::error::Error>> {
  if query.len() == 0 {
    return Ok("".into());
  }
  let query = Regex::new("\n+$").unwrap().replace_all(query, "");
  let valid_format = Regex::new(r"^((?>[^=\n\s]+=[^=\n]+)\n?)+$").unwrap().is_match(&query).unwrap();
  if !valid_format {
    return Err(Box::from("Not valid query format"));
  }
  let query = Regex::new("\n").unwrap().replace_all(&query, "&");
  Ok(query.to_string())
}

fn replace_env_variables(input: &str, values: &HashMap<String, String>) -> String {
  let re = Regex::new(r"\{\{([a-zA-Z_-]+)\}\}").unwrap();
  let mut output = input.to_string();

  for capture in re.captures_iter(input) {
    let capture_matches = capture.unwrap();
    let key = capture_matches.get(1).unwrap().as_str();
    let value = match values.get(key) {
      Some(v) => v.to_string(),
      None => String::new(),
    };
    output = output.replace(capture_matches.get(0).unwrap().as_str(), &value);
  }

  output
}

fn render_popup<'a>(servers: &Vec<String>, user_input: &UserInput) -> List<'a> {
  let servers_block = Block::default()
    .borders(Borders::ALL)
    .style(focused_style(&user_input, MenuItem::ServerListPopup))
    .title("Servers")
    .border_type(BorderType::Plain);

  // let url_socket = Regex::new(r"^https?://[^/]*").unwrap();
  // let url_socket = Regex::new(r"^https?://.*?/").unwrap();
  let items: Vec<_> = servers
    .iter()
    .map(|svr| {
      // let parsed_owned_req_path = url_socket.replace(&req.path.clone(),"").clone().to_string();
      ListItem::new(Line::from(vec![
        Span::styled(svr.clone(), Style::default()),
        // Span::styled(parsed_owned_req_path, Style::default(),)
        // Span::styled(req.url.clone().replace(r"http://", ""), Style::default(),)
      ]))
    })
    .collect();

  let list = List::new(items).block(servers_block).highlight_style(
    Style::default()
            // .bg(Color::Yellow)
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
  );

  list
}

//fn update_user_input(user_input: &mut UserInput, new_sel: &CRequest, servers: &Servers) {
//  user_input.method = new_sel.method;
//
//  user_input.server.drain(..);
//  user_input.server.push_str(servers.get_active());
//
//  user_input.path.drain(..);
//  user_input.path.push_str(&new_sel.path[..]);
//
//  user_input.query.drain(..);
//  user_input.query.push_str(&new_sel.query[..]);
//
//  user_input.payload.drain(..);
//  user_input.payload.push_str(&new_sel.payload[..]);
//
//  user_input.headers.drain(..);
//  user_input.headers.push_str(&new_sel.headers[..]);
//
//  user_input.parsing_rules.drain(..);
//  user_input.parsing_rules.push_str(&new_sel.parsing_rules[..]);
//}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
  let popup_layout = Layout::default()
    .direction(Direction::Vertical)
    .constraints(
      [
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
      ]
      .as_ref(),
    )
    .split(r);

  Layout::default()
    .direction(Direction::Horizontal)
    .constraints(
      [
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
      ]
      .as_ref(),
    )
    .split(popup_layout[1])[1]
}

fn is_editable(item: MenuItem) -> bool {
  match item {
    MenuItem::Query => true,
    MenuItem::Payload => true,
    MenuItem::Headers => true,
    MenuItem::JsonPath => true,
    _ => false,
  }
}
