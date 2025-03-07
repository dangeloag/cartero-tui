use std::sync::OnceLock;
use std::{collections::HashMap, fs};

use ratatui::widgets::ListState;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::components::home::server;

const DB_PATH: &str = "./cartero.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct RequestInput {
  pub method: server::HttpMethod,
  pub server: String,
  pub path: String,
  pub query: String,
  pub payload: String,
  pub headers: String,
  #[serde(default = "emtpy_string")]
  pub parsing_rules: String,
}

impl Default for RequestInput {
  fn default() -> Self {
    RequestInput {
      method: server::HttpMethod::GET,
      server: String::from("http://localhost"),
      path: String::new(),
      query: String::new(),
      payload: String::new(),
      headers: String::new(),
      parsing_rules: String::new(),
    }
  }
}

fn emtpy_string() -> String {
  String::from("")
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalStorageRepository {
  #[serde(default = "default_env")]
  env: HashMap<String, String>,
  servers: Servers,
  requests: Requests,
}

fn default_env() -> HashMap<String, String> {
  HashMap::new()
}

impl LocalStorageRepository {
  pub fn new() -> LocalStorageRepository {
    match fs::read_to_string(DB_PATH) {
      Ok(db_content) => match serde_json::from_str::<LocalStorageRepository>(&db_content) {
        Ok(data) => data,
        Err(e) => {
          error!("{:?}", e);
          LocalStorageRepository {
            env: default_env(),
            servers: Servers { value: vec![String::from("http://localhost")], active: 0 },
            requests: Requests::default(),
          }
        },
      },
      Err(e) => {
        error!("{:?}", e);
        LocalStorageRepository {
          env: default_env(),
          servers: Servers { value: vec![String::from("http://localhost")], active: 0 },
          requests: Requests::default(),
        }
      },
    }
  }

  pub fn save(&self) {
    write_db(self.clone());
  }

  pub fn get_method(&self) -> server::HttpMethod {
    self.requests.get_active().method
  }

  pub fn get_server(&self) -> String {
    self.servers.get_active()
  }

  pub fn push_to_server(&mut self, c: char) {
    self.servers.handle_char(c);
  }

  pub fn pop_server(&mut self) {
    self.servers.pop()
  }

  pub fn clear_server(&mut self) {
    self.servers.clear();
  }

  pub fn get_query(&self) -> String {
    self.requests.get_active().query
  }

  pub fn get_payload(&self) -> String {
    self.requests.get_active().payload
  }

  pub fn get_headers(&self) -> String {
    self.requests.get_active().headers
  }

  pub fn get_path(&self) -> String {
    self.requests.get_active().path
  }

  pub fn set_next_method(&mut self) {
    self.requests.set_next_method()
  }

  pub fn set_previous_method(&mut self) {
    self.requests.set_previous_method()
  }

  pub fn push_to_path(&mut self, c: char) {
    self.requests.push_to_path(c);
  }

  pub fn pop_path(&mut self) {
    self.requests.pop_path()
  }

  pub fn clear_path(&mut self) {
    self.requests.clear_path();
  }

  pub fn push_to_querystring(&mut self, c: char) {
    self.requests.push_to_querystring(c);
  }

  pub fn pop_querystring(&mut self) {
    self.requests.pop_querystring()
  }

  pub fn clear_querystring(&mut self) {
    self.requests.clear_querystring();
  }

  pub fn push_to_payload(&mut self, c: char) {
    self.requests.push_to_payload(c);
  }

  pub fn pop_payload(&mut self) {
    self.requests.pop_payload()
  }

  pub fn clear_payload(&mut self) {
    self.requests.clear_payload();
  }

  pub fn push_to_headers(&mut self, c: char) {
    self.requests.push_to_headers(c);
  }

  pub fn pop_headers(&mut self) {
    self.requests.pop_headers()
  }

  pub fn clear_headers(&mut self) {
    self.requests.clear_headers();
  }

  pub fn get_request_list(&self) -> &Vec<RequestInput> {
    &self.requests.get_request_list()
  }

  pub fn get_active_request_idx(&self) -> usize {
    self.requests.active
  }

  pub fn add_request(&mut self) {
    self.requests.add(RequestInput::default());
  }

  pub fn delete_request(&mut self) {
    self.requests.delete_active();
  }

  pub fn duplicate_request(&mut self) {
    self.requests.add(self.requests.get_active());
  }

  pub fn next_request(&mut self) {
    self.requests.next();
  }

  pub fn previous_request(&mut self) {
    self.requests.previous();
  }
}

impl Default for LocalStorageRepository {
  fn default() -> Self {
    // Use get_or_init to initialize the singleton within the default method
    debug!("Starting LocalStorageRepository");
    match fs::read_to_string(DB_PATH) {
      Ok(db_content) => match serde_json::from_str::<LocalStorageRepository>(&db_content) {
        Ok(data) => data,
        Err(e) => {
          error!("{:?}", e);
          LocalStorageRepository {
            env: default_env(),
            servers: Servers { value: vec![String::from("http://localhost")], active: 0 },
            requests: Requests::default(),
          }
        },
      },
      Err(e) => {
        error!("{:?}", e);
        LocalStorageRepository {
          env: default_env(),
          servers: Servers { value: vec![String::from("http://localhost")], active: 0 },
          requests: Requests::default(),
        }
      },
    }
  }
}

#[derive(Serialize, Deserialize, Clone)]
struct Requests {
  value: Vec<RequestInput>,
  active: usize,
}

impl Default for Requests {
  fn default() -> Self {
    Requests { value: vec![RequestInput::default()], active: 0 }
  }
}

impl Requests {
  fn get_active(&self) -> RequestInput {
    if self.value.len() > 0 {
      self.value[self.active].clone()
    } else {
      RequestInput::default()
    }
  }

  fn get_active_mut(&mut self) -> &mut RequestInput {
    &mut self.value[self.active]
  }

  fn update_active(&mut self, s: RequestInput) {
    self.value[self.active] = s;
  }

  fn set_active(&mut self, idx: usize) {
    // let idx = self.value.iter().position(|x| *x == s).unwrap_or(0);
    self.active = idx;
  }

  fn add(&mut self, s: RequestInput) {
    self.value.push(s);
  }

  fn delete_active(&mut self) {
    self.value.remove(self.active);

    // Adjust active index
    if self.value.is_empty() || self.active == 0 {
      // If the list is empty after removal, set active to None or reset it
      self.active = 0;
    } else {
      // If the last item was deleted, update the active index to the new last item
      self.active -= 1;
    }
  }

  fn next(&mut self) {
    let length = self.value.len();
    if length == 0 {
      return;
    }

    if self.active == length - 1 {
      self.active = 0;
    } else {
      self.active += 1;
    }
  }

  fn previous(&mut self) {
    let length = self.value.len();
    if length == 0 {
      return;
    }

    if self.active == 0 {
      self.active = length - 1;
    } else {
      self.active -= 1;
    }
  }

  fn set_next_method(&mut self) {
    self.get_active_mut().method = self.get_active_mut().method.next()
  }

  fn set_previous_method(&mut self) {
    self.get_active_mut().method = self.get_active_mut().method.previous()
  }

  fn push_to_path(&mut self, c: char) {
    self.get_active_mut().path.push(c)
  }

  fn pop_path(&mut self) {
    self.get_active_mut().path.pop();
  }

  fn clear_path(&mut self) {
    self.get_active_mut().path.clear();
  }

  fn push_to_querystring(&mut self, c: char) {
    self.get_active_mut().query.push(c)
  }

  fn pop_querystring(&mut self) {
    self.get_active_mut().query.pop();
  }

  fn clear_querystring(&mut self) {
    self.get_active_mut().query.clear();
  }

  fn push_to_payload(&mut self, c: char) {
    self.get_active_mut().payload.push(c)
  }

  fn pop_payload(&mut self) {
    self.get_active_mut().payload.pop();
  }

  fn clear_payload(&mut self) {
    self.get_active_mut().payload.clear();
  }

  fn push_to_headers(&mut self, c: char) {
    self.get_active_mut().headers.push(c)
  }

  fn pop_headers(&mut self) {
    self.get_active_mut().headers.pop();
  }

  fn clear_headers(&mut self) {
    self.get_active_mut().headers.clear();
  }

  fn get_request_list(&self) -> &Vec<RequestInput> {
    &self.value
  }
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct Servers {
  value: Vec<String>,
  active: usize,
}

impl Servers {
  fn get_active(&self) -> String {
    self.value[self.active].clone()
  }

  fn get_active_mut(&mut self) -> &mut String {
    &mut self.value[self.active]
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

  fn handle_char(&mut self, c: char) {
    self.get_active_mut().push(c);
  }

  fn pop(&mut self) {
    self.get_active_mut().pop();
  }

  fn clear(&mut self) {
    self.get_active_mut().clear();
  }
}

//fn read_db() -> LocalStorageRepository {
//  match fs::read_to_string(DB_PATH) {
//    Ok(db_content) => match serde_json::from_str::<LocalStorageRepository>(&db_content) {
//      Ok(data) => data,
//      Err(e) => {
//        error!("{:?}", e);
//        LocalStorageRepository::default()
//      },
//    },
//    Err(e) => {
//      error!("{:?}", e);
//      LocalStorageRepository::default()
//    },
//  }
//}

use serde_json::to_string_pretty;

fn write_db(data: LocalStorageRepository) {
  let serialized_data = to_string_pretty(&data).expect("Can be serialized");
  debug!("Serialized data: {}", serialized_data); // Print the serialized data
  fs::write(DB_PATH, serialized_data).expect("Can write to database");
}
