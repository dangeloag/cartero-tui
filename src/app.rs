use std::sync::{atomic::AtomicBool, mpsc as stdMpsc, Arc, Mutex};

use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use futures::executor::block_on;
use ratatui::prelude::Rect;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
  action::Action,
  components::{fps::FpsCounter, home::Home, Component},
  config::Config,
  repository::{self, local_storage::LocalStorageRepository},
  tui,
};

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
  #[default]
  Home,
}

pub struct App {
  pub config: Config,
  pub tick_rate: f64,
  pub frame_rate: f64,
  pub components: Vec<Box<dyn Component>>,
  pub should_quit: bool,
  pub should_suspend: bool,
  pub mode: Mode,
  pub last_tick_key_events: Vec<KeyEvent>,
  pub vim_is_open: bool,
  //pub repository: repository::local_storage::LocalStorageRepository,
}

impl App {
  pub fn new(tick_rate: f64, frame_rate: f64) -> Result<Self> {
    let repo = LocalStorageRepository::default();
    let home = Home::new(Arc::new(Mutex::new(repo)));
    let fps = FpsCounter::new();
    let config = Config::new()?;
    let mode = Mode::Home;
    Ok(Self {
      tick_rate,
      frame_rate,
      components: vec![Box::new(home), Box::new(fps)],
      should_quit: false,
      should_suspend: false,
      config,
      mode,
      last_tick_key_events: Vec::new(),
      vim_is_open: false,
    })
  }

  pub async fn run(&mut self) -> Result<()> {
    let (action_tx, mut action_rx) = mpsc::unbounded_channel();
    // let (vim_tx, mut vim_rx) = mpsc::unbounded_channel();

    let mut tui = tui::Tui::new()?;
    tui.tick_rate(self.tick_rate);
    tui.frame_rate(self.frame_rate);
    tui.enter()?;

    for component in self.components.iter_mut() {
      component.register_action_handler(action_tx.clone())?;
    }

    for component in self.components.iter_mut() {
      component.register_config_handler(self.config.clone())?;
    }

    for component in self.components.iter_mut() {
      component.init()?;
    }

    loop {
      if let Some(e) = tui.next().await {
        match self.mode {
          Mode::Home => {
            match e {
              tui::Event::Quit => action_tx.send(Action::Quit)?,
              tui::Event::Tick => action_tx.send(Action::Tick)?,
              tui::Event::Render => action_tx.send(Action::Render)?,
              tui::Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
              tui::Event::Key(key) => {
                for component in self.components.iter_mut() {
                  if let Some(action) = component.handle_events(Some(e.clone()))? {
                    action_tx.send(action)?;
                  }
                }

                // if let Some(keymap) = self.config.keybindings.get(&self.mode) {
                //   if let Some(action) = keymap.get(&vec![key.clone()]) {
                //     log::info!("Got action: {action:?}");
                //     action_tx.send(action.clone())?;
                //   } else {
                //     // If the key was not handled as a single key action,
                //     // then consider it for multi-key combinations.
                //     self.last_tick_key_events.push(key);
                //     // Check for multi-key combinations
                //     if let Some(action) = keymap.get(&self.last_tick_key_events) {
                //       log::info!("Got action: {action:?}");
                //       action_tx.send(action.clone())?;
                //     }
                //   }
                // };
              },
              _ => {},
            }
          },
          _ => {},
          // Mode::Insert => match e {
          //   tui::Event::Key(key) => match key.code {
          //     crossterm::event::KeyCode::Esc => self.mode = Mode::Command,
          //     _ => {
          //       for component in self.components.iter_mut() {
          //         if let Some(action) = component.handle_events(Some(e.clone()))? {
          //           action_tx.send(action)?;
          //         }
          //         action_tx.send(Action::Render)?;
          //       }
          //     },
          //   },
          //   event => {
          //     log::info!("Received unkown event {event:?}");
          //   },
          // },
        }
      }

      while let Ok(action) = action_rx.try_recv() {
        if action != Action::Tick && action != Action::Render {
          log::debug!("{action:?}");
        }
        match action {
          Action::Tick => {
            self.last_tick_key_events.drain(..);
          },
          Action::Quit => self.should_quit = true,
          Action::Suspend => self.should_suspend = true,
          Action::Resume => {
            self.should_suspend = false;
            self.vim_is_open = false
          },
          Action::FocusLost => {
            tui.exit()?;
            for component in self.components.iter_mut() {
              if let Some(action) = component.update(Action::EditInput)? {
                action_tx.send(action)?
              };
            }
          },
          Action::FocusGained => {
            tui = tui::Tui::new()?;
            tui.tick_rate(self.tick_rate);
            tui.frame_rate(self.frame_rate);
            tui.enter()?;
          },
          Action::Resize(w, h) => {
            tui.resize(Rect::new(0, 0, w, h))?;
            tui.draw(|f| {
              for component in self.components.iter_mut() {
                let r = component.draw(f, f.size());
                if let Err(e) = r {
                  action_tx.send(Action::Error(format!("Failed to draw: {:?}", e))).unwrap();
                }
              }
            })?;
          },
          Action::Render => {
            tui.draw(|f| {
              for component in self.components.iter_mut() {
                let r = component.draw(f, f.size());
                if let Err(e) = r {
                  action_tx.send(Action::Error(format!("Failed to draw: {:?}", e))).unwrap();
                }
              }
            })?;
          },
          _ => {},
        }
        for component in self.components.iter_mut() {
          if let Some(action) = component.update(action.clone())? {
            action_tx.send(action)?
          };
        }
      }
      if self.should_suspend {
        tui.suspend()?;
        action_tx.send(Action::Resume)?;
        tui = tui::Tui::new()?;
        tui.tick_rate(self.tick_rate);
        tui.frame_rate(self.frame_rate);
        tui.enter()?;
      } else if self.should_quit {
        tui.stop()?;
        break;
      }
    }
    tui.exit()?;
    Ok(())
  }
}
