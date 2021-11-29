use chrono::prelude::*;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use fancy_regex::Regex;
use reqwest::{blocking::Response, header::{HeaderMap, HeaderName, HeaderValue}};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, fs::File, io, str::FromStr};
use std::io::prelude::*;
use std::thread;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tui::{Terminal, backend::CrosstermBackend, layout::{Alignment, Constraint, Direction, Layout}, style::{Color, Modifier, Style}, text::{Span, Spans}, widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph}};


enum Event<I> {
    Input(I),
    Tick,
}

#[derive(Serialize, Deserialize, Clone)]
struct Pet {
    id: usize,
    name: String,
    category: String,
    age: usize,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone)]
struct UserInput {
    method: HttpMethod,
    url: String,
    query: String,
    payload: String,
    headers: String,
    active_menu_item: MenuItem
}

impl From<CRequest> for UserInput {
    fn from(c_req : CRequest) -> Self {
        UserInput {
            url: c_req.url,
            query: c_req.query,
            payload: c_req.payload,
            headers: c_req.headers,
            method: c_req.method,
            active_menu_item: MenuItem::BaseUrl
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct CRequest {
    method: HttpMethod,
    url: String,
    query: String,
    payload: String,
    headers: String,
}

impl Default for CRequest {
    fn default() -> Self {
        CRequest {
            method: HttpMethod::GET,
            url: String::from("http://localhost:3000/rust"),
            query: String::new(),
            payload: String::new(),
            headers: String::new(),
        }
    }
}

struct AppOutput {
    response_headers: String,
    response_payload: String
}

impl Default for AppOutput {
    fn default() -> Self {
        AppOutput {
            response_headers: String::new(),
            response_payload: String::new()
        }
    }
}

impl UserInput {
    fn get_active(&mut self) -> &mut String{
        match self.active_menu_item {
            MenuItem::BaseUrl => &mut self.url,
            MenuItem::Query => &mut self.query,
            MenuItem::Payload => &mut self.payload,
            MenuItem::Headers => &mut self.headers,
            _=> panic!("Not implemented")
        }
    }
}

impl Default for UserInput {
    fn default() -> Self {
        UserInput {
            method: HttpMethod::GET,
            url: String::from("http://localhost:3000/rust"),
            query: String::new(),
            payload: String::new(),
            headers: String::new(),
            active_menu_item: MenuItem::BaseUrl,
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
enum MenuItem {
    BaseUrl,
    Query,
    Payload,
    Headers,
    Requests
}

impl MenuItem {
    fn next(&self) -> Self {
        match self {
            Self::BaseUrl => Self::Query,
            Self::Query => Self::Payload,
            Self::Payload => Self::Headers,
            Self::Headers => Self::Requests,
            Self::Requests => Self::BaseUrl
        }
    }

    fn previous(&self) -> Self {
        match self {
            Self::BaseUrl => Self::Requests,
            Self::Query => Self::BaseUrl,
            Self::Payload => Self::Query,
            Self::Headers => Self::Payload,
            Self::Requests => Self::Headers
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE
}

impl HttpMethod {
    fn to_string(&self) -> String {
        match  self {
            Self::GET => String::from("GET"),
            Self::POST => String::from("POST"),
            Self::PUT => String::from("PUT"),
            Self::DELETE => String::from("DELETE"),
        }
    }

    fn previous(&self) -> Self {
        match  self {
            Self::GET => Self::DELETE,
            Self::POST => Self::GET,
            Self::PUT => Self::POST,
            Self::DELETE => Self::PUT
        }
    }

    fn next(&self) -> Self {
        match  self {
            Self::GET => Self::POST,
            Self::POST => Self::PUT,
            Self::PUT => Self::DELETE,
            Self::DELETE => Self::GET
        }
    }

    fn get_style(&self) -> Style {
        match  self {
            Self::GET => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
            Self::POST =>Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            Self::PUT => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            Self::DELETE => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        }
    }

}


impl From<MenuItem> for usize {
    fn from(input: MenuItem) -> usize {
        match input {
            MenuItem::BaseUrl => 0,
            MenuItem::Query => 1,
            MenuItem::Payload => 2,
            MenuItem::Headers => 3,
            MenuItem::Requests => 4,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {

    enable_raw_mode().expect("can run in raw mode");
    let (user_req, user_requests) = load_requests();
    let mut user_input = UserInput::from(user_req);
    let mut app_output = AppOutput::default();

    let (tx, rx) = mpsc::channel();
    let tick_rate = Duration::from_millis(200);
    thread::spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout).expect("poll works") {
                if let CEvent::Key(key) = event::read().expect("can read events") {
                    tx.send(Event::Input(key)).expect("can send events");
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
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut req_list_state = ListState::default();
    req_list_state.select(Some(0));
    terminal.clear()?;

    loop {
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
                .constraints([Constraint::Length(12), Constraint::Min(50)].as_ref())
                .split(chunks[0]);

            let method = Paragraph::new(user_input.method.to_string())
                .style(user_input.method.get_style())
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                    .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
                    .style(focused_style(&user_input, MenuItem::BaseUrl))
                    .title("---Method--")
                    .border_type(BorderType::Plain),
                    );
            rect.render_widget(method, method_and_url_chunk[0]);

            let base_url = Paragraph::new(user_input.url.as_ref())
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                    .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
                    .style(focused_style(&user_input, MenuItem::BaseUrl))
                    .title("---baseUrl")
                    .border_type(BorderType::Plain),
                    );
            rect.render_widget(base_url, method_and_url_chunk[1]);

            let request_chunk = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [Constraint::Length(30), Constraint::Length(50), Constraint::Min(50)].as_ref()
                    )
                .split(chunks[1]);

            let requests_list = render_reqs(&user_requests, &user_input);

            rect.render_stateful_widget(requests_list, request_chunk[0], &mut req_list_state);

            let request_data_chunk = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [Constraint::Percentage(30),
                    Constraint::Percentage(35),
                    Constraint::Percentage(35)]
                    .as_ref()
                    )
                .split(request_chunk[1]);

            let request_result_chunk = Layout::default()
                .direction(Direction::Vertical)
                .constraints( [Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
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
            rect.render_widget(copyright, chunks[2]);

            match user_input.active_menu_item {
                MenuItem::BaseUrl =>  {
                    let (x_offset, y_offset) = parse_coord(&user_input.url);
                    rect.set_cursor(
                        // method_and_url_chunk[1].x + user_input.url.len() as u16 + 0,
                        // method_and_url_chunk[1].y + 1,
                        method_and_url_chunk[1].x + x_offset as u16 - 1,
                        method_and_url_chunk[1].y + y_offset as u16,
                        );
                },
                MenuItem::Query => {
                    let (x_offset, y_offset) = parse_coord(&user_input.query);
                    rect.set_cursor(
                        request_data_chunk[0].x + x_offset,
                        request_data_chunk[0].y + y_offset,
                        )
                },
                MenuItem::Payload => {
                    let (x_offset, y_offset) = parse_coord(&user_input.payload);
                    rect.set_cursor(
                        request_data_chunk[1].x + x_offset,
                        request_data_chunk[1].y + y_offset,
                        )
                },
                MenuItem::Headers => {
                    let (x_offset, y_offset) = parse_coord(&user_input.headers);
                    rect.set_cursor(
                        request_data_chunk[2].x + x_offset,
                        request_data_chunk[2].y + y_offset,
                        )
                },
                _ => {}
            }

        })?;

        match rx.recv()? {
            Event::Input(event) => match event {
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Enter
                } if user_input.active_menu_item == MenuItem::BaseUrl => {
                    process_request(&user_input, &mut app_output);
                },
                /////////////////////////////////////////////////////////////////////////
                //                               GLOBAL                                //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Enter
                } => {
                    process_request(&user_input, &mut app_output);
                },
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Down
                } if user_input.active_menu_item == MenuItem::BaseUrl => {
                    user_input.method = user_input.method.next()
                },
                KeyEvent {
                    modifiers: _,
                    code: KeyCode::Up
                } if user_input.active_menu_item == MenuItem::BaseUrl => {
                    user_input.method = user_input.method.previous()
                },
                /////////////////////////////////////////////////////////////////////////
                //                               GLOBAL - ALT                          //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('1')
                } => {
                    user_input.active_menu_item = MenuItem::BaseUrl;
                },
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('2')
                } => {
                    user_input.active_menu_item = MenuItem::Query;
                },
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('3')
                } => {
                    user_input.active_menu_item = MenuItem::Payload
                },
                KeyEvent {
                    modifiers: KeyModifiers::ALT,
                    code: KeyCode::Char('4')
                } => {
                    user_input.active_menu_item = MenuItem::Headers
                },
                /////////////////////////////////////////////////////////////////////////
                //                          GLOBAL - CONTROL                           //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('q')
                } => {
                    disable_raw_mode()?;
                    terminal.show_cursor()?;
                    break;
                },
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: KeyCode::Char('u')
                } => {
                    user_input.get_active().drain(..);
                },
                KeyEvent {
                    modifiers: KeyModifiers::CONTROL,
                    code: _
                } => {},
                /////////////////////////////////////////////////////////////////////////
                //                      GLOBAL- NO MODIFIERS                           //
                /////////////////////////////////////////////////////////////////////////
                KeyEvent { modifiers: _, code: KeyCode::Tab} => {
                    user_input.active_menu_item = user_input.active_menu_item.next();
                },
                KeyEvent { modifiers: _, code: KeyCode::BackTab} => {
                    user_input.active_menu_item = user_input.active_menu_item.previous();
                },
                KeyEvent { modifiers: _, code: KeyCode::Char(c) } => {
                    user_input.get_active().push(c);
                },
                KeyEvent { modifiers: _, code: KeyCode::Enter } => {
                    user_input.get_active().push_str("\n");
                },
                KeyEvent { modifiers: _, code: KeyCode::Backspace} => {
                    user_input.get_active().pop();
                },
                _ => {}
            },
            Event::Tick => {}
        }

    }
    Ok(())
}

fn process_request(input_data: &UserInput,  app_output: &mut AppOutput) {
    let response_result = send_request(&input_data);
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
            let json_result : Result<Value, _> = serde_json::from_str(&response_text);
            if let Ok(json) = json_result {
                json.serialize(&mut ser).unwrap();
                app_output.response_payload= String::from_utf8(ser.into_inner()).unwrap();
            } else {
                app_output.response_payload = String::from(response_text)
            }
        },
        Err(shoot) => {
            app_output.response_payload = shoot.to_string();
        }
    }
}

fn send_request( input_data: &UserInput ) -> Result<Response, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let query = parse_query(&input_data.query)?;
    let url = format!("{}?{}", &input_data.url, query);
    let headers : HeaderMap = parse_headers(&input_data.headers)?;
    match input_data.method {
        HttpMethod::GET => {
            let  response = client.get(url)
                .headers(headers)
                .send()?;
            Ok(response)
        },
        HttpMethod::POST => {
            let payload : &str = &input_data.payload;
            let body;
            if payload.len() > 0 {
                let filename = format!("/tmp/{}-{}{}.json", &input_data.method.to_string(), Utc::now().format("%Y%m%d_"), &Utc::now().format("%s").to_string()[4..]);
                let mut file = File::create(&filename)?;
                file.write(input_data.payload.as_bytes())?;
                body = std::fs::File::open(filename)?;
            } else {
                body = std::fs::File::create("/tmp/empty.body")?;
            }
            let res = client.post(url)
                .headers(headers)
                .body(body);
            let response = res.send()?;
            Ok(response)
        },
        _ => Err(Box::from(format!("Method {} not implemented yet", input_data.method.to_string())))
    }
}

fn focused_style(user_input: &UserInput, item: MenuItem) -> Style {
    if user_input.active_menu_item == item {
        Style::default().fg(Color::Rgb(51, 255, 207))
    } else {
        Style::default().fg(Color::White)
    }
}

fn parse_coord(text: &str) -> ( u16, u16 ) {
    let list : Vec<&str> = text.split("\n").collect();
    let x_offset = list.last().unwrap().len() as u16 + 1;
    let y_offset = list.len() as u16;
    (x_offset, y_offset)
}

fn parse_query(query: &str) -> Result<String, Box<dyn std::error::Error>> {
    if query.len() == 0 { return Ok("".into()) }
    let query = Regex::new("\n+$").unwrap()
        .replace_all(query, "");
    let valid_format = Regex::new(r"^((?>[^=\n\s]+=[^=\n]+)\n?)+$").unwrap().is_match(&query).unwrap();
    if !valid_format { return Err(Box::from("Not valid query format")); }
    let query = Regex::new("\n").unwrap()
        .replace_all(&query, "&");
    Ok(query.to_string())
}

fn parse_headers(headers: &str) -> Result<HeaderMap, Box<dyn std::error::Error>> {
    if headers.len() == 0 { return Ok(HeaderMap::new()) }
    let headers = Regex::new("\n+$") .unwrap()
        .replace_all(headers, "");
    let valid_format = Regex::new(r"^((?>[^:\n\s]+\s?:[^:\n]+)\n?)+$").unwrap().is_match(&headers).unwrap();
    if !valid_format { return Err(Box::from("Not valid header format")); }
    let headers : Vec<(&str, &str)> = headers
        .split("\n")
        .map(|h| {
            let kv : Vec<&str> = h.split(":")
                .map(|v| v.trim()).collect();
            (kv[0], kv[1])

        }).collect();
    let mut header_map = HeaderMap::new();
    headers
        .iter()
        .for_each(|(name, value)| {
            header_map.insert(
                HeaderName::from_str(name).unwrap(),
                HeaderValue::from_str(value).unwrap()
                );
        });
    Ok(header_map)
}


const DB_PATH: &str = "./cartero.json";

fn op(_: std::io::Error) -> Result<String,Box<dyn std::error::Error>> {
    Ok(String::from("[]"))
}

fn read_db() -> Result<Vec<CRequest>, Box<dyn std::error::Error>> {
    let db_content = fs::read_to_string(DB_PATH).or_else(op)?;
    let parsed: Vec<CRequest> = serde_json::from_str(&db_content)?;
    Ok(parsed)
}

fn load_requests() -> (CRequest, Vec<CRequest>) {
    let req_list = read_db().expect("can fetch request list");
    let items: Vec<_> = req_list
        .iter()
        .map(|req| {
            ListItem::new(Spans::from(vec![Span::styled(
                        req.url.clone(),
                        Style::default(),
                        )]))
        })
        .collect();

    let request = if req_list.len() > 0 {
        req_list.get(0).expect("exists").clone()
    } else {
        CRequest::default()
    };

    (request, req_list)

}

fn render_reqs<'a>(user_reqs : &Vec<CRequest>, user_input: &UserInput) -> List<'a> {
    let requests = Block::default()
        .borders(Borders::ALL)
        // .style(Style::default().fg(Color::White))
        .style(focused_style(&user_input, MenuItem::Requests))
        .title("Requests")
        .border_type(BorderType::Plain);

    let items: Vec<_> = user_reqs
        .iter()
        .map(|req| {
            ListItem::new(Spans::from(vec![Span::styled(
                req.url.clone(),
                Style::default(),
            )]))
        })
        .collect();

    let list = List::new(items).block(requests).highlight_style(
        Style::default()
            .bg(Color::Yellow)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    );

    list
}
