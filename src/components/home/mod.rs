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
        server::HttpMethod::PUT => {
          req_builder = Some(client.put(url).headers(headers).body(payload));
        },
        server::HttpMethod::DELETE => {
          req_builder = Some(client.delete(url).headers(headers).body(payload));
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

    Ok(())
  }
}

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

fn is_focused(active_item: MenuItem, item: MenuItem) -> bool {
  active_item == item
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
