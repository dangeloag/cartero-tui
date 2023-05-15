use chrono::prelude::*;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use fancy_regex::Regex;
use reqwest::{
    blocking::Response,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    fs::{self, OpenOptions},
    io::{self},
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{io::prelude::*, process::Stdio};
use std::{str::FromStr, sync::mpsc};
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use std::{sync::Arc, thread};
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Terminal,
};

enum Event<I> {
    Input(I),
    Tick,
}

#[derive(Serialize, Deserialize, Clone)]
struct Servers {
    value: Vec<String>,
    active: usize,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_file_path = Path::new("app.log");
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)?;

    log_message(&mut log_file, "La puta que te pario");

    enable_raw_mode().expect("can run in raw mode");
    let mut local_storage: LocalStorage = read_db().unwrap();
    // let servers = local_storage.servers;
    let (user_req, _) = load_requests();
    let mut user_input = UserInput::from(user_req);
    let mut app_output = AppOutput::default();
    let mut popup: bool = false;
    let mut parsing_rules_popup: bool = false;
    let vim_running = Arc::new(AtomicBool::new(false));
    let vim_running_loop_ref = vim_running.clone();
    let jq_is_installed = jq_is_installed();

    let (tx, rx) = mpsc::channel();
    let (vim_tx, vim_rx) = mpsc::channel();
    let tick_rate = Duration::from_millis(200);
    thread::spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout).expect("poll works") {
                if let CEvent::Key(key) = event::read().expect("can read events") {
                    if vim_running_loop_ref.load(std::sync::atomic::Ordering::Relaxed) {
                        vim_rx.recv().unwrap();
                    } else {
                        tx.send(Event::Input(key)).expect("can send events");
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                if let Ok(_) = tx.send(Event::Tick) {
                    last_tick = Instant::now();
                }
            }
        }
    });

    let stdout = io::stdout();
    // execute!(stdout, EnterAlternateScreen);
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut req_list_state = ListState::default();
    let mut server_list_state = ListState::default();
    let servers = read_db().unwrap().servers;
    req_list_state.select(Some(0));
    server_list_state.select(Some(servers.active));
    terminal.clear()?;

    loop {
        let (_, user_requests) = load_requests();
        terminal.draw(|rect| {
            let size = rect.size();
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
                .split(size);

            let method_and_url_chunk = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Length(user_input.method.to_string().len() as u16 + 10),
                        Constraint::Length(user_input.server.len() as u16 + 2),
                        Constraint::Min(50),
                    ]
                    .as_ref(),
                )
                .split(chunks[0]);

            let method = Paragraph::new(user_input.method.to_string())
                .style(user_input.method.get_style())
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
                        .style(focused_style(&user_input, MenuItem::Server))
                        .title("---Method--")
                        .border_type(BorderType::Plain),
                );
            rect.render_widget(method, method_and_url_chunk[0]);

            let base_url = Paragraph::new(user_input.server.as_ref())
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::TOP | Borders::BOTTOM)
                        .style(focused_style(&user_input, MenuItem::Server))
                        .title("server")
                        .border_type(BorderType::Plain),
                );
            rect.render_widget(base_url, method_and_url_chunk[1]);

            let path = Paragraph::new(user_input.path.as_ref())
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
                        .style(focused_style(&user_input, MenuItem::Path))
                        .title("path")
                        .border_type(BorderType::Plain),
                );
            rect.render_widget(path, method_and_url_chunk[2]);

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

            let requests_list = render_reqs(&user_requests, &user_input);

            rect.render_stateful_widget(requests_list, request_chunk[0], &mut req_list_state);

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

            let query = Paragraph::new(user_input.query.as_ref())
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(focused_style(&user_input, MenuItem::Query))
                        .title("Query")
                        .border_type(BorderType::Plain),
                );

            rect.render_widget(query, request_data_chunk[0]);

            let payload = Paragraph::new(user_input.payload.as_ref())
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(focused_style(&user_input, MenuItem::Payload))
                        .title("Payload")
                        .border_type(BorderType::Plain),
                );
            rect.render_widget(payload, request_data_chunk[1]);

            let headers = Paragraph::new(user_input.headers.as_ref())
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(focused_style(&user_input, MenuItem::Headers))
                        .title("Headers")
                        .border_type(BorderType::Plain),
                );
            rect.render_widget(headers, request_data_chunk[2]);

            let result_headers = Paragraph::new(app_output.response_headers.as_ref())
                .style(Style::default().fg(Color::Green))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::White))
                        .title("Response Headers")
                        .border_type(BorderType::Plain),
                );
            rect.render_widget(result_headers, request_result_chunk[0]);

            let result_payload = Paragraph::new(app_output.response_payload.as_ref())
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::White))
                        .title("Response Payload")
                        .border_type(BorderType::Plain),
                );
            rect.render_widget(result_payload, request_result_chunk[1]);

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
            rect.render_widget(copyright, lower_bar_chunks[0]);

            let json_path_title = if jq_is_installed {
                "JQ"
            } else {
                "Serde Pointer"
            };

            let response_json_path = Paragraph::new(user_input.json_path.as_ref())
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(focused_style(&user_input, MenuItem::JsonPath))
                        .title(json_path_title)
                        .border_type(BorderType::Plain),
                );
            rect.render_widget(response_json_path, lower_bar_chunks[1]);

            if popup {
                let requests_list2 = render_popup(&local_storage.servers.value, &user_input);
                let _ = Block::default().title("Popup").borders(Borders::ALL);
                let area = centered_rect(60, 20, size);
                rect.render_widget(Clear, area); //this clears out the background
                rect.render_stateful_widget(requests_list2, area, &mut server_list_state);
                user_input.active_menu_item = MenuItem::ServerListPopup;
            }

            if parsing_rules_popup {
                let _ = Block::default().title("Popup").borders(Borders::ALL);
                let area = centered_rect(50, 30, size);
                let rules_text = Paragraph::new(user_input.parsing_rules.as_ref())
                    .style(Style::default().fg(Color::LightCyan))
                    .alignment(Alignment::Left)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .style(focused_style(&user_input, MenuItem::ParsingRulesPopup))
                            .title("Parsing Rules")
                            .border_type(BorderType::Plain),
                    );

                rect.render_widget(Clear, area); //this clears out the background
                rect.render_widget(rules_text, area);
                user_input.active_menu_item = MenuItem::ParsingRulesPopup;
            }

            match user_input.active_menu_item {
                MenuItem::Server => {
                    let (x_offset, y_offset) = parse_coord(&user_input.server);
                    rect.set_cursor(
                        // method_and_url_chunk[1].x + user_input.url.len() as u16 + 0,
                        // method_and_url_chunk[1].y + 1,
                        method_and_url_chunk[1].x + x_offset as u16 - 1,
                        method_and_url_chunk[1].y + y_offset as u16,
                    );
                }
                MenuItem::Path => {
                    let (x_offset, y_offset) = parse_coord(&user_input.path);
                    rect.set_cursor(
                        // method_and_url_chunk[1].x + user_input.url.len() as u16 + 0,
                        // method_and_url_chunk[1].y + 1,
                        method_and_url_chunk[2].x + x_offset as u16 - 1,
                        method_and_url_chunk[2].y + y_offset as u16,
                    );
                }
                MenuItem::Query => {
                    let (x_offset, y_offset) = parse_coord(&user_input.query);
                    rect.set_cursor(
                        request_data_chunk[0].x + x_offset,
                        request_data_chunk[0].y + y_offset,
                    )
                }
                MenuItem::Payload => {
                    let (x_offset, y_offset) = parse_coord(&user_input.payload);
                    rect.set_cursor(
                        request_data_chunk[1].x + x_offset,
                        request_data_chunk[1].y + y_offset,
                    )
                }
                MenuItem::Headers => {
                    let (x_offset, y_offset) = parse_coord(&user_input.headers);
                    rect.set_cursor(
                        request_data_chunk[2].x + x_offset,
                        request_data_chunk[2].y + y_offset,
                    )
                }
                MenuItem::JsonPath => {
                    let (x_offset, y_offset) = parse_coord(&user_input.json_path);
                    rect.set_cursor(
                        lower_bar_chunks[1].x + x_offset,
                        lower_bar_chunks[1].y + y_offset,
                    )
                }
                _ => {}
            }
        })?;

        match rx.recv()? {
            Event::Input(event) => match event {
                /////////////////////////////////////////////////////////////////////////
                //                         SUPER GLOBAL                                //
                /////////////////////////////////////////////////////////////////////////
                // KeyEvent {
                //     modifiers: _,
                //     code: KeyCode::PageDown,
                // } => result_payload.scroll((10,10)),
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Tab,
                } => {
                    user_input.active_menu_item = user_input.active_menu_item.next();
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::BackTab,
                } => {
                    user_input.active_menu_item = user_input.active_menu_item.previous();
                }
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('1'),
                } => {
                    user_input.active_menu_item = MenuItem::Server;
                }
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('2'),
                } => {
                    user_input.active_menu_item = MenuItem::Requests;
                }
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('3'),
                } => {
                    user_input.active_menu_item = MenuItem::Query;
                }
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('4'),
                } => user_input.active_menu_item = MenuItem::Payload,
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('5'),
                } => user_input.active_menu_item = MenuItem::Headers,

                /////////////////////////////////////////////////////////////////////////
                //                          SERVER/PATH MENU                             //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Enter,
                } if user_input.active_menu_item == MenuItem::Server
                    || user_input.active_menu_item == MenuItem::Path
                    || user_input.active_menu_item == MenuItem::Requests =>
                {
                    process_request(&user_input, &mut app_output, &mut local_storage);
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Down,
                }
                | KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('j'),
                } if user_input.active_menu_item == MenuItem::Server
                    || user_input.active_menu_item == MenuItem::Path =>
                {
                    user_input.method = user_input.method.next()
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Up,
                }
                | KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('k'),
                } if user_input.active_menu_item == MenuItem::Server
                    || user_input.active_menu_item == MenuItem::Path =>
                {
                    user_input.method = user_input.method.previous()
                }
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('l'),
                } if user_input.active_menu_item == MenuItem::Server
                    || user_input.active_menu_item == MenuItem::Path =>
                {
                    popup = !popup;
                }
                // KeyEvent {
                //     modifiers: _,
                //     code: KeyCode::Char(c),
                // } if user_input.active_menu_item == MenuItem::Server => {
                //     user_input.get_active().push(c);
                //     local_storage
                //         .servers
                //         .update_active(user_input.get_active().to_owned())
                // }
                // KeyEvent {
                //     modifiers: KeyModifiers::CONTROL,
                //     code: KeyCode::Char('s'),
                // } if user_input.active_menu_item == MenuItem::Server
                //     || user_input.active_menu_item == MenuItem::Path =>
                // {
                //     local_storage
                //         .servers
                //         .update_active(String::from(&user_input.server));
                //     write_db(local_storage.clone());
                // }
                // KeyEvent {
                //     modifiers: KeyModifiers::CONTROL,
                //     code: KeyCode::Char('h'),
                // } if user_input.active_menu_item == MenuItem::Server
                //     || user_input.active_menu_item == MenuItem::Path =>
                // {
                //     local_storage
                //         .servers
                //         .set_active(String::from(&user_input.server[..]));
                //     write_db(local_storage.clone());
                // }
                /////////////////////////////////////////////////////////////////////////
                //                              PARSING POPUP MENU                           //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Char('q') | KeyCode::Esc,
                } if user_input.active_menu_item == MenuItem::ParsingRulesPopup => {
                    user_input.active_menu_item = MenuItem::Path;
                    parsing_rules_popup = false;
                }
                /////////////////////////////////////////////////////////////////////////
                //                              POPUP MENU                           //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Char('q') | KeyCode::Esc,
                } if user_input.active_menu_item == MenuItem::ServerListPopup => {
                    user_input.active_menu_item = MenuItem::Path;
                    popup = false;
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Enter,
                } if user_input.active_menu_item == MenuItem::ServerListPopup => {
                    let new_selected = server_list_state.selected().unwrap();
                    user_input.server = local_storage
                        .servers
                        .value
                        .get(new_selected)
                        .unwrap()
                        .into();
                    local_storage.servers.active = new_selected;
                    user_input.active_menu_item = MenuItem::Path;
                    popup = false;
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Down | KeyCode::Char('j'),
                } if user_input.active_menu_item == MenuItem::ServerListPopup => {
                    let amount_svrs = local_storage.servers.value.len();
                    let mut new_selection = 0;
                    if amount_svrs > 0 {
                        if let Some(selected) = server_list_state.selected() {
                            if selected >= amount_svrs - 1 {
                                new_selection = 0;
                            } else {
                                new_selection = selected + 1;
                            }
                            server_list_state.select(Some(new_selection));
                            local_storage.servers.set_active(new_selection);
                        }
                    }
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Up | KeyCode::Char('k'),
                } if user_input.active_menu_item == MenuItem::ServerListPopup => {
                    if let Some(selected) = server_list_state.selected() {
                        let amount_svrs = local_storage.servers.value.len();
                        let mut new_selection = 0;
                        if selected == 0 {
                            new_selection = amount_svrs - 1;
                        } else {
                            new_selection = selected - 1;
                        }
                        server_list_state.select(Some(new_selection));
                        local_storage.servers.set_active(new_selection);
                    }
                }
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('a') | KeyCode::Esc,
                } if user_input.active_menu_item == MenuItem::ServerListPopup => {
                    local_storage.servers.add_server(String::from("localhost"));
                    write_db(local_storage.clone());
                }
                KeyEvent {
                    modifiers: _,
                    code: _,
                } if user_input.active_menu_item == MenuItem::ServerListPopup => {}
                /////////////////////////////////////////////////////////////////////////
                //                              REQUEST MENU                           //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Char('a'),
                } if user_input.active_menu_item == MenuItem::Requests => {
                    add_request(&user_input, &mut local_storage);
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Char('d'),
                } if user_input.active_menu_item == MenuItem::Requests => {
                    delete_request(&user_input, &mut local_storage, &mut req_list_state);
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Down | KeyCode::Char('j'),
                } if user_input.active_menu_item == MenuItem::Requests
                    || user_input.active_menu_item == MenuItem::ServerListPopup =>
                {
                    if let Some(selected) = req_list_state.selected() {
                        // TODO: read from db?
                        let amount_pets = user_requests.len();
                        if selected >= amount_pets - 1 {
                            req_list_state.select(Some(0));
                        } else {
                            req_list_state.select(Some(selected + 1))
                        }
                        let new_sel = user_requests
                            .get(req_list_state.selected().unwrap())
                            .unwrap()
                            .clone();
                        update_user_input(&mut user_input, &new_sel, &local_storage.servers);
                    }
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Up | KeyCode::Char('k'), // } if user_input.active_menu_item == MenuItem::Requests => {
                } if user_input.active_menu_item == MenuItem::Requests
                    || user_input.active_menu_item == MenuItem::ServerListPopup =>
                {
                    if let Some(selected) = req_list_state.selected() {
                        // TODO: read from db?
                        let amount_pets = user_requests.len();
                        if selected == 0 {
                            req_list_state.select(Some(amount_pets - 1));
                        } else {
                            req_list_state.select(Some(selected - 1))
                        }
                        let new_sel = user_requests
                            .get(req_list_state.selected().unwrap())
                            .unwrap()
                            .clone();
                        update_user_input(&mut user_input, &new_sel, &local_storage.servers);
                    }
                }
                /////////////////////////////////////////////////////////////////////////
                //                      PAYLOAD MENU                                   //
                /////////////////////////////////////////////////////////////////////////
                /////////////////////////////////////////////////////////////////////////
                //                               GLOBAL - ALT                          //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Enter,
                } => {
                    process_request(&user_input, &mut app_output, &mut local_storage);
                }
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('s'),
                } => {}
                /////////////////////////////////////////////////////////////////////////
                //                          GLOBAL - CONTROL                           //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('e'),
                } if is_editable(user_input.active_menu_item) => {
                    let mut temp_file = tempfile::NamedTempFile::new()?;
                    if user_input.get_active().len() > 0 {
                        temp_file.write_all(user_input.get_active().as_bytes())?;
                    } else {
                        temp_file.write_all("{\n\"\" : \"\"\n}".as_bytes())?;
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
                    vim_running.store(true, std::sync::atomic::Ordering::Relaxed);
                    let vim_cmd_result = output.wait().expect("Run exits ok");
                    vim_tx.send(1).unwrap();
                    vim_running.store(false, std::sync::atomic::Ordering::Relaxed);

                    if !vim_cmd_result.success() {
                        return Err(format!(
                            "Vim exited with status code {}",
                            vim_cmd_result.code().unwrap_or(-1)
                        )
                        .into());
                    }

                    let mut edited_text = String::new();
                    std::fs::File::open(temp_file.path())?.read_to_string(&mut edited_text)?;

                    user_input.get_active().clear();
                    user_input.get_active().push_str(edited_text.as_str());
                }
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('r'),
                } => parsing_rules_popup = true,
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('s'),
                } => {
                    if let Some(selected) = req_list_state.selected() {
                        // let mut data = read_db().expect("Can read database");
                        let cq = CRequest::from(user_input.clone());
                        local_storage.requests[selected] = cq;
                        // let _ = std::mem::replace(&mut data.requests[selected], cq);
                        local_storage
                            .servers
                            .update_active(user_input.server.to_owned());
                        local_storage.servers = local_storage.servers;
                        // let _ = std::mem::replace(&mut data.servers, local_storage.servers.clone());
                        write_db(local_storage.clone());
                        // local_storage = data
                    }
                }
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('q'),
                } => {
                    disable_raw_mode()?;
                    terminal.clear()?;
                    terminal.show_cursor()?;
                    break;
                }
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('u'),
                } => {
                    user_input.get_active().drain(..);
                }
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: _,
                } => {}
                /////////////////////////////////////////////////////////////////////////
                //                             GLOBAL IGNORE                           //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: _,
                    code: _,
                } if user_input.active_menu_item == MenuItem::Requests => {}
                /////////////////////////////////////////////////////////////////////////
                //                      GLOBAL- NO MODIFIERS                           //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Char(c),
                } => {
                    user_input.get_active().push(c);
                    if user_input.active_menu_item == MenuItem::Server {
                        local_storage
                            .servers
                            .update_active(user_input.get_active().to_owned())
                    }
                    if user_input.active_menu_item == MenuItem::JsonPath {
                        if jq_is_installed {
                            parse_with_jq(&mut app_output, &mut user_input, &mut log_file)
                        } else {
                            parse_with_serde(&mut app_output, &mut user_input)
                        }
                    }
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Enter,
                } => {
                    user_input.get_active().push_str("\n");
                }
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Backspace,
                } => {
                    user_input.get_active().pop();
                }
                _ => {}
            },
            Event::Tick => {}
        }
    }
    Ok(())
}

fn process_request(
    input_data: &UserInput,
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
    input_data: &UserInput,
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
    headers: &str,
    env: &HashMap<String, String>,
) -> Result<HeaderMap, Box<dyn std::error::Error>> {
    if headers.len() == 0 {
        return Ok(HeaderMap::new());
    }
    let headers_with_env = replace_env_variables(headers, env);
    let headers = Regex::new("\n+$")
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
            HeaderName::from_str(name).unwrap(),
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
            ListItem::new(Spans::from(vec![
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
            ListItem::new(Spans::from(vec![
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
