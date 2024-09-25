use std::{collections::HashMap, sync::Arc, time::Duration};
use chrono::prelude::*;
use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use fancy_regex::Regex;
use futures::{executor::block_on, StreamExt};
use log::error;
use ratatui::{prelude::*, widgets::*};
use std::{io::prelude::*, process::Stdio,
    process::Command,
    fs::File,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::tempfile;
use tokio::sync::mpsc::{Sender, UnboundedReceiver, UnboundedSender};
use reqwest::{
    blocking::Response,
    header::{HeaderMap, HeaderName, HeaderValue},
};
// use tokio::sync::mpsc::UnboundedSender;
use std::sync::mpsc::Receiver;
use tracing::trace;
use tui_input::{backend::crossterm::EventHandler, Input};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Component, Frame};
use crate::{action::Action, config::key_event_to_string};

#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub enum Mode {
  #[default]
  Normal,
  Insert,
  Processing,
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
  pub editor_rx: Option<UnboundedReceiver<Action>>,
  pub keymap: HashMap<KeyEvent, Action>,
  pub text: Vec<String>,
  pub last_events: Vec<KeyEvent>,
  pub req_list_state: ListState,
  pub server_list_state: ListState,
  pub servers: Servers,
  pub user_req: CRequest,
  pub user_input: UserInput,
  pub app_output: AppOutput,
  pub jq_is_installed: bool,
  pub popup: bool,
  pub local_storage: LocalStorage,
  pub parsing_rules_popup: bool,
}

impl Home {
  pub fn new() -> Self {
    Self::default()
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

  pub fn add(&mut self, s: String) {
    self.text.push(s)
  }

  pub fn schedule_increment(&mut self, i: usize) {
    let tx = self.action_tx.clone().unwrap();
    tokio::spawn(async move {
      tx.send(Action::EnterProcessing).unwrap();
      tokio::time::sleep(Duration::from_secs(1)).await;
      tx.send(Action::Increment(i)).unwrap();
      tx.send(Action::ExitProcessing).unwrap();
    });
  }

  pub fn schedule_decrement(&mut self, i: usize) {
    let tx = self.action_tx.clone().unwrap();
    tokio::spawn(async move {
      tx.send(Action::EnterProcessing).unwrap();
      tokio::time::sleep(Duration::from_secs(1)).await;
      tx.send(Action::Decrement(i)).unwrap();
      tx.send(Action::ExitProcessing).unwrap();
    });
  }

  pub fn increment(&mut self, i: usize) {
    self.counter = self.counter.saturating_add(i);
  }

  pub fn decrement(&mut self, i: usize) {
    self.counter = self.counter.saturating_sub(i);
  }
}

impl Component for Home {
  fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
    self.action_tx = Some(tx);
    Ok(())
  }

  fn handle_key_events(&mut self, key: KeyEvent) -> Result<Option<Action>> {
    self.last_events.push(key.clone());
    let action = match self.mode {
      Mode::Normal | Mode::Processing => return Ok(None),
      Mode::Insert => match key.code {
        KeyCode::Esc => Action::EnterNormal,
        KeyCode::Enter => {
          if let Some(sender) = &self.action_tx {
            if let Err(e) = sender.send(Action::CompleteInput(self.input.value().to_string())) {
              error!("Failed to send action: {:?}", e);
            }
          }
          Action::EnterNormal
        },
        KeyCode::Char('e') => { Action::FocusLost },
        _ => {
          self.input.handle_event(&crossterm::event::Event::Key(key));
          Action::Update
        },
      },
    };
    Ok(Some(action))
  }

  fn update(&mut self, action: Action) -> Result<Option<Action>> {
    match action {
      Action::Tick => self.tick(),
      Action::Render => self.render_tick(),
      Action::ToggleShowHelp if self.mode == Mode::Normal => self.show_help = !self.show_help,
      Action::ScheduleIncrement if self.mode != Mode::Insert => self.schedule_increment(1),
      Action::ScheduleDecrement if self.mode != Mode::Insert => self.schedule_decrement(1),
      Action::Increment(i) => self.increment(i),
      Action::Decrement(i) => self.decrement(i),
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
      Action::CompleteInput(s) => {
        self.add(s);
        self.input.reset()
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

    // let mut req_list_state = ListState::default();
    // let mut server_list_state = ListState::default();
    // let servers = read_db().unwrap().servers;
    // req_list_state.select(Some(0));
    // server_list_state.select(Some(servers.active));
    // let (user_req, _) = load_requests();
    // let mut user_input = UserInput::from(user_req);
    let (_, user_requests) = load_requests();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Min(2),
                        Constraint::Length(3),
                    ]
                    .as_ref(),
                )
                .split(rect);

            let method_and_url_chunk = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Length(self.user_input.method.to_string().len() as u16 + 10),
                        Constraint::Length(self.user_input.server.len() as u16 + 2),
                        Constraint::Min(50),
                    ]
                    .as_ref(),
                )
                .split(chunks[0]);

            let method = Paragraph::new(self.user_input.method.to_string())
                .style(self.user_input.method.get_style())
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
                        .style(focused_style(&self.user_input, MenuItem::Server))
                        .title("---Method--")
                        .border_type(BorderType::Plain),
                );
            f.render_widget(method, method_and_url_chunk[0]);

            let base_url = Paragraph::new(AsRef::<str>::as_ref(&self.user_input.server))
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::TOP | Borders::BOTTOM)
                        .style(focused_style(&self.user_input, MenuItem::Server))
                        .title("server")
                        .border_type(BorderType::Plain),
                );
            f.render_widget(base_url, method_and_url_chunk[1]);

            let path = Paragraph::new(AsRef::<str>::as_ref(&self.user_input.path))
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
                        .style(focused_style(&self.user_input, MenuItem::Path))
                        .title("path")
                        .border_type(BorderType::Plain),
                );
            f.render_widget(path, method_and_url_chunk[2]);

            let request_chunk = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Length(30),
                        Constraint::Length(50),
                        Constraint::Min(50),
                    ]
                    .as_ref(),
                )
                .split(chunks[1]);

            let requests_list = render_reqs(&user_requests, &self.user_input);

            f.render_stateful_widget(requests_list, request_chunk[0], &mut self.req_list_state);

            let request_data_chunk = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(30),
                        Constraint::Percentage(35),
                        Constraint::Percentage(35),
                    ]
                    .as_ref(),
                )
                .split(request_chunk[1]);

            let request_result_chunk = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
                .split(request_chunk[2]);

            let query = Paragraph::new(AsRef::<str>::as_ref(&self.user_input.query))
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(focused_style(&self.user_input, MenuItem::Query))
                        .title("Query")
                        .border_type(BorderType::Plain),
                );

            f.render_widget(query, request_data_chunk[0]);

            let payload = Paragraph::new(AsRef::<str>::as_ref(&self.user_input.payload))
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(focused_style(&self.user_input, MenuItem::Payload))
                        .title("Payload")
                        .border_type(BorderType::Plain),
                );
            f.render_widget(payload, request_data_chunk[1]);

            let headers = Paragraph::new(AsRef::<str>::as_ref(&self.user_input.headers))
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(focused_style(&self.user_input, MenuItem::Headers))
                        .title("Headers")
                        .border_type(BorderType::Plain),
                );
            f.render_widget(headers, request_data_chunk[2]);

            let result_headers = Paragraph::new(AsRef::<str>::as_ref(&self.app_output.response_headers))
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

            let result_payload = Paragraph::new(AsRef::<str>::as_ref(&self.app_output.response_payload))
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
                .split(chunks[2]);

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

            let json_path_title = if self.jq_is_installed {
                "JQ"
            } else {
                "Serde Pointer"
            };

            let response_json_path = Paragraph::new(AsRef::<str>::as_ref(&self.user_input.json_path))
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(focused_style(&self.user_input, MenuItem::JsonPath))
                        .title(json_path_title)
                        .border_type(BorderType::Plain),
                );
            f.render_widget(response_json_path, lower_bar_chunks[1]);

            if self.popup {
                let requests_list2 = render_popup(&self.local_storage.servers.value, &self.user_input);
                let _ = Block::default().title("Popup").borders(Borders::ALL);
                let area = centered_rect(60, 20, rect);
                f.render_widget(Clear, area); //this clears out the background
                f.render_stateful_widget(requests_list2, area, &mut self.server_list_state);
                self.user_input.active_menu_item = MenuItem::ServerListPopup;
            }

            if self.parsing_rules_popup {
                let _ = Block::default().title("Popup").borders(Borders::ALL);
                let area = centered_rect(50, 30, rect);
                let rules_text = Paragraph::new(AsRef::<str>::as_ref(&self.user_input.parsing_rules))
                    .style(Style::default().fg(Color::LightCyan))
                    .alignment(Alignment::Left)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(focused_style(&self.user_input, MenuItem::ParsingRulesPopup))
                            .title("Parsing Rules")
                            .border_type(BorderType::Plain),
                    );

                f.render_widget(Clear, area); //this clears out the background
                f.render_widget(rules_text, area);
                self.user_input.active_menu_item = MenuItem::ParsingRulesPopup;
            }

            match self.user_input.active_menu_item {
                MenuItem::Server => {
                    let (x_offset, y_offset) = parse_coord(&self.user_input.server);
                    f.set_cursor(
                        // method_and_url_chunk[1].x + self.user_input.url.len() as u16 + 0,
                        // method_and_url_chunk[1].y + 1,
                        method_and_url_chunk[1].x + x_offset as u16 - 1,
                        method_and_url_chunk[1].y + y_offset as u16,
                    );
                }
                MenuItem::Path => {
                    let (x_offset, y_offset) = parse_coord(&self.user_input.path);
                    f.set_cursor(
                        // method_and_url_chunk[1].x + self.user_input.url.len() as u16 + 0,
                        // method_and_url_chunk[1].y + 1,
                        method_and_url_chunk[2].x + x_offset as u16 - 1,
                        method_and_url_chunk[2].y + y_offset as u16,
                    );
                }
                MenuItem::Query => {
                    let (x_offset, y_offset) = parse_coord(&self.user_input.query);
                    f.set_cursor(
                        request_data_chunk[0].x + x_offset,
                        request_data_chunk[0].y + y_offset,
                    )
                }
                MenuItem::Payload => {
                    let (x_offset, y_offset) = parse_coord(&self.user_input.payload);
                    f.set_cursor(
                        request_data_chunk[1].x + x_offset,
                        request_data_chunk[1].y + y_offset,
                    )
                }
                MenuItem::Headers => {
                    let (x_offset, y_offset) = parse_coord(&self.user_input.headers);
                    f.set_cursor(
                        request_data_chunk[2].x + x_offset,
                        request_data_chunk[2].y + y_offset,
                    )
                }
                MenuItem::JsonPath => {
                    let (x_offset, y_offset) = parse_coord(&self.user_input.json_path);
                    f.set_cursor(
                        lower_bar_chunks[1].x + x_offset,
                        lower_bar_chunks[1].y + y_offset,
                    )
                }
                _ => {}
            }

    Ok(())
  }
}


#[derive(Serialize, Deserialize, Clone)]
struct Servers {
    value: Vec<String>,
    active: usize,
}

impl Default for Servers {
    fn default() -> Self {
        read_db().unwrap().servers
    }
}

impl Servers {
    fn get_active(&self) -> &str {
        &self.value[self.active]
    }

    fn update_active(&mut self, s: String) {
        self.value[self.active] = s;
    }

    fn set_active(&mut self, idx: usize) {
        // let idx = self.value.iter().position(|x| *x == s).unwrap_or(0);
        self.active = idx;
    }

    fn add_server(&mut self, s: String) {
        self.value.push(s.clone());
        self.set_active(self.active + 1);
    }

    fn next(&mut self) {}
}

#[derive(Serialize, Deserialize, Clone)]
struct UserInput {
    method: HttpMethod,
    server: String,
    path: String,
    query: String,
    payload: String,
    json_path: String,
    headers: String,
    parsing_rules: String,
    active_menu_item: MenuItem,
}

impl From<CRequest> for UserInput {
    fn from(c_req: CRequest) -> Self {
        UserInput {
            server: c_req.server,
            path: c_req.path,
            query: c_req.query,
            payload: c_req.payload,
            json_path: String::new(),
            headers: c_req.headers,
            parsing_rules: c_req.parsing_rules,
            method: c_req.method,
            active_menu_item: MenuItem::Server,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct CRequest {
    method: HttpMethod,
    server: String,
    path: String,
    query: String,
    payload: String,
    headers: String,
    #[serde(default = "emtpy_string")]
    parsing_rules: String,
}

fn emtpy_string() -> String {
    String::from("")
}

fn default_env() -> HashMap<String, String> {
    HashMap::new()
}

#[derive(Serialize, Deserialize, Clone)]
struct LocalStorage {
    #[serde(default = "default_env")]
    env: HashMap<String, String>,
    servers: Servers,
    requests: Vec<CRequest>,
}

impl Default for LocalStorage {
    fn default() -> Self {
        read_db().unwrap()
    }
}

impl From<UserInput> for CRequest {
    fn from(user_input: UserInput) -> Self {
        CRequest {
            server: user_input.server,
            path: user_input.path,
            query: user_input.query,
            payload: user_input.payload,
            headers: user_input.headers,
            method: user_input.method,
            parsing_rules: user_input.parsing_rules,
        }
    }
}

impl Default for CRequest {
    fn default() -> Self {
        CRequest {
            method: HttpMethod::GET,
            server: String::from("http://localhost"),
            path: String::new(),
            query: String::new(),
            payload: String::new(),
            headers: String::new(),
            parsing_rules: String::new(),
        }
    }
}

struct AppOutput {
    response_headers: String,
    response_payload: String,
    response_payload_last: String,
}

impl Default for AppOutput {
    fn default() -> Self {
        AppOutput {
            response_headers: String::new(),
            response_payload: String::new(),
            response_payload_last: String::new(),
        }
    }
}

impl UserInput {
    fn get_active(&mut self) -> &mut String {
        match self.active_menu_item {
            MenuItem::Server => &mut self.server,
            MenuItem::Path => &mut self.path,
            MenuItem::Query => &mut self.query,
            MenuItem::Payload => &mut self.payload,
            MenuItem::Headers => &mut self.headers,
            MenuItem::ServerListPopup => &mut self.server,
            MenuItem::ParsingRulesPopup => &mut self.parsing_rules,
            MenuItem::JsonPath => &mut self.json_path,
            _ => panic!("Not implemented"),
        }
    }
}

impl Default for UserInput {
    fn default() -> Self {
        UserInput {
            method: HttpMethod::GET,
            server: String::from("http://localhost"),
            path: String::new(),
            query: String::new(),
            payload: String::new(),
            json_path: String::new(),
            headers: String::new(),
            parsing_rules: String::new(),
            active_menu_item: MenuItem::Server,
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
enum MenuItem {
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

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
}

impl HttpMethod {
    fn to_string(&self) -> String {
        match self {
            Self::GET => String::from("GET"),
            Self::POST => String::from("POST"),
            Self::PUT => String::from("PUT"),
            Self::DELETE => String::from("DELETE"),
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

    fn get_style(&self) -> Style {
        match self {
            Self::GET => Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            Self::POST => Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            Self::PUT => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Self::DELETE => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
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

fn process_request(
    input_data: &'static UserInput,
    app_output: &mut AppOutput,
    local_storage: &mut LocalStorage,
) {
    let response_result = send_request(&input_data, &local_storage.env);
    match response_result {
        Ok(response) => {
            app_output.response_headers = format!("{:?}", response.headers())
                .replace("\",", "\n")
                .replace("{", " ")
                .replace("}", "");
            let buf = Vec::new();
            let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
            let mut ser = serde_json::Serializer::with_formatter(buf, formatter);
            let response_text = &response.text().expect("Valid text response");
            let json_result: Result<Value, _> = serde_json::from_str(&response_text);
            if let Ok(json) = json_result {
                json.serialize(&mut ser).unwrap();
                app_output.response_payload = String::from_utf8(ser.into_inner()).unwrap();
            } else {
                app_output.response_payload = String::from(response_text)
            }
            app_output.response_payload_last = app_output.response_payload.clone();
            let parsin_rules = parse_rules(&input_data.parsing_rules[..]);

            for rule in parsin_rules {
                match serde_json::from_str::<Value>(response_text) {
                    Ok(json_value) => {
                        if let Some(field_value) = json_value.pointer(&rule.1[..]) {
                            local_storage
                                .env
                                .insert(rule.0, field_value.clone().as_str().unwrap().into());
                            write_db(local_storage.clone());
                        }
                    }
                    _ => (),
                }
            }
        }
        Err(shoot) => {
            app_output.response_payload = shoot.to_string();
        }
    }
}

// fn extract_and_save_matching_result(
//     word: &str,
//     json_path: &str,
//     results_map: &mut HashMap<String, String>,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     let json_value: Value = serde_json::from_str(&response_body)?;
//
//     if let Some(field_value) = json_value.pointer(json_path).unwrap().as_str() {
//         if field_value.contains(word) {
//             results_map.insert(json_path.to_owned(), field_value.to_owned());
//         }
//     }
//
//     Ok(())
// }

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

fn send_request(
    input_data: &'static UserInput,
    env: &HashMap<String, String>,
) -> Result<Response, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let query = parse_query(&input_data.query)?;
    let url = format!("{}{}?{}", &input_data.server, &input_data.path, query);
    let headers: HeaderMap = parse_headers(&input_data.headers, env)?;
    match input_data.method {
        HttpMethod::GET => {
            let response = client.get(url).headers(headers).send()?;
            Ok(response)
        }
        HttpMethod::POST => {
            let payload: &str = &input_data.payload;
            let body;
            if payload.len() > 0 {
                let filename = format!(
                    "/tmp/{}-{}{}.json",
                    &input_data.method.to_string(),
                    Utc::now().format("%Y%m%d_"),
                    &Utc::now().format("%s").to_string()[4..]
                );
                let mut file = File::create(&filename)?;
                file.write(input_data.payload.as_bytes())?;
                body = std::fs::File::open(filename)?;
            } else {
                body = std::fs::File::create("/tmp/empty.body")?;
            }
            let res = client.post(url).headers(headers).body(body);
            let response = res.send()?;
            Ok(response)
        }
        _ => Err(Box::from(format!(
            "Method {} not implemented yet",
            input_data.method.to_string()
        ))),
    }
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
    let valid_format = Regex::new(r"^((?>[^=\n\s]+=[^=\n]+)\n?)+$")
        .unwrap()
        .is_match(&query)
        .unwrap();
    if !valid_format {
        return Err(Box::from("Not valid query format"));
    }
    let query = Regex::new("\n").unwrap().replace_all(&query, "&");
    Ok(query.to_string())
}

fn parse_headers(
    headers: &'static str,
    env: &HashMap<String, String>,
) -> Result<HeaderMap, Box<dyn std::error::Error>> {
    if headers.len() == 0 {
        return Ok(HeaderMap::new());
    }
    let headers_with_env = replace_env_variables(headers, env).clone();
    let new_headers = Regex::new("\n+$")
        .unwrap()
        .replace_all(&headers_with_env, "");
    // TODO: move regex compiltion out  of function
    let valid_format = Regex::new(r"^((?>[^:\n\s]+\s?:[^:\n]+)\n?)+$")
        .unwrap()
        .is_match(&headers)
        .unwrap();
    if !valid_format {
        return Err(Box::from("Not valid header format"));
    }
    let headers: Vec<(&str, &str)> = headers
        .split("\n")
        .map(|h| {
            let kv: Vec<&str> = h.split(":").map(|v| v.trim()).collect();
            (kv[0], kv[1])
        })
        .collect();
    let mut header_map = HeaderMap::new();
    headers.iter().for_each(|(name, value)| {
        header_map.insert(
            HeaderName::from_static(name),
            HeaderValue::from_str(value).unwrap(),
        );
    });
    Ok(header_map)
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

const DB_PATH: &str = "./cartero.json";

fn default_config(_: std::io::Error) -> Result<String, Box<dyn std::error::Error>> {
    Ok(String::from(
        r#"
    {
    "env": {},
     "servers": {
         "value": ["http://localhost"],
         "active": 0
     },
        "requests": [
        {
            "method": "GET",
            "server": "",
            "path": "",
            "query": "",
            "payload": "",
            "headers": ""

        }]
    }"#,
    ))
}

fn read_db() -> Result<LocalStorage, Box<dyn std::error::Error>> {
    let db_content = fs::read_to_string(DB_PATH).or_else(default_config)?;
    // let parsed: Vec<CRequest> = serde_json::from_str(&db_content)?;
    let parsed: LocalStorage = serde_json::from_str(&db_content)?;
    Ok(parsed)
}

fn load_requests() -> (CRequest, Vec<CRequest>) {
    let req_list = read_db().expect("can fetch request list").requests;

    let request = if req_list.len() > 0 {
        req_list.get(0).expect("exists").clone()
    } else {
        CRequest::default()
    };

    (request, req_list)
}

fn render_reqs<'a>(user_reqs: &Vec<CRequest>, user_input: &UserInput) -> List<'a> {
    let requests = Block::default()
        .borders(Borders::ALL)
        .style(focused_style(&user_input, MenuItem::Requests))
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
                Span::styled(req.path.clone(), Style::default()), // Span::styled(parsed_owned_req_path, Style::default(),)
                                                                  // Span::styled(req.url.clone().replace(r"http://", ""), Style::default(),)
            ]))
        })
        .collect();

    let list = List::new(items).block(requests).highlight_style(
        Style::default()
            // .bg(Color::Yellow)
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    list
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

fn add_request(_: &UserInput, data: &mut LocalStorage) {
    let new_req = CRequest::default();
    data.requests.push(new_req);
    write_db(data.clone());
}

fn delete_request(_: &UserInput, data: &mut LocalStorage, req_list: &mut ListState) {
    let selected_idx = req_list.selected().unwrap_or(0);
    data.requests.remove(selected_idx);
    if selected_idx == data.requests.len() {
        req_list.select(Some(selected_idx - 1));
    }
    write_db(data.clone());
}

fn write_db(data: LocalStorage) {
    let buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(buf, formatter);
    data.serialize(&mut ser).expect("Can be serialized");
    fs::write(DB_PATH, ser.into_inner()).expect("Can write to database");
}

fn update_user_input(user_input: &mut UserInput, new_sel: &CRequest, servers: &Servers) {
    user_input.method = new_sel.method;

    user_input.server.drain(..);
    user_input.server.push_str(servers.get_active());

    user_input.path.drain(..);
    user_input.path.push_str(&new_sel.path[..]);

    user_input.query.drain(..);
    user_input.query.push_str(&new_sel.query[..]);

    user_input.payload.drain(..);
    user_input.payload.push_str(&new_sel.payload[..]);

    user_input.headers.drain(..);
    user_input.headers.push_str(&new_sel.headers[..]);

    user_input.parsing_rules.drain(..);
    user_input
        .parsing_rules
        .push_str(&new_sel.parsing_rules[..]);
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

fn is_editable(item: MenuItem) -> bool {
    match item {
        MenuItem::Query => true,
        MenuItem::Payload => true,
        MenuItem::Headers => true,
        MenuItem::JsonPath => true,
        _ => false,
    }
}

fn parse_with_serde(app_output: &mut AppOutput, user_input: &mut UserInput) {
    match serde_json::from_str::<Value>(&app_output.response_payload_last) {
        Ok(json_value) => {
            if let Some(field_value) = json_value.pointer(user_input.get_active()) {
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
                    }
                    Value::String(s) => s.to_string(),
                    Value::Null => "null".to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => field_value.to_string(),
                };
                app_output.response_payload = field_str;
            } else {
                app_output.response_payload = app_output.response_payload_last.clone();
            }
        }
        _ => (),
    }

    // match serde_json::from_str::<Value>(&app_output.response_payload_last) {
    //     Ok(json_value) => {
    //         if let Some(field_value) =
    //             json_value.pointer(user_input.get_active())
    //         {
    //             app_output.response_payload =
    //                 field_value.as_str().unwrap_or_else(|| "".into()).into();
    //         } else {
    //             app_output.response_payload =
    //                 app_output.response_payload_last.clone();
    //         }
    //     }
    //     _ => (),
    // }
}

fn parse_with_jq(app_output: &mut AppOutput, user_input: &mut UserInput, log_file: &mut File) {
    let filter = &user_input.json_path;
    let jq_cmd = format!(
        "echo -E '{}' | jq '{}'",
        app_output.response_payload_last, filter
    );

    // log_message(log_file, &jq_cmd);
    let filtered_str = Command::new("bash")
        .arg("-c")
        .arg(jq_cmd)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .unwrap();

    // Check if there was an error with the jq command
    if !filtered_str.status.success() {
        app_output.response_payload = app_output.response_payload_last.clone();
        log_message(log_file, &String::from_utf8(filtered_str.stderr).unwrap());
        // log_message(
        //     log_file,
        //     &format!(
        //         "Filter: {}\nRequest: {}",
        //         filter,
        //         app_output.response_payload_last.clone()
        //     ),
        // );
    } else {
        let filtered_response_str = String::from_utf8(filtered_str.stdout).unwrap();
        app_output.response_payload = filtered_response_str;
    }
}

fn jq_is_installed() -> bool {
    match Command::new("jq").arg("--version").output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

fn log_message(log_file: &mut File, message: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    writeln!(log_file, "{} - {}", timestamp, message);
}
