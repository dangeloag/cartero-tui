use std::{fs::File, path::PathBuf};

use color_eyre::eyre::Result;
use directories::ProjectDirs;
use lazy_static::lazy_static;
use tracing_error::ErrorLayer;
use tracing_subscriber::{self, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, Layer};

use tracing::{debug, error, info, warn};
use tracing_subscriber::FmtSubscriber;

pub static GIT_COMMIT_HASH: &'static str = env!("RATATUI_COUNTER_GIT_INFO");

lazy_static! {
  pub static ref PROJECT_NAME: String = env!("CARGO_CRATE_NAME").to_uppercase().to_string();
  pub static ref DATA_FOLDER: Option<PathBuf> =
    std::env::var(format!("{}_DATA", PROJECT_NAME.clone())).ok().map(PathBuf::from);
  pub static ref CONFIG_FOLDER: Option<PathBuf> =
    std::env::var(format!("{}_CONFIG", PROJECT_NAME.clone())).ok().map(PathBuf::from);
  pub static ref LOG_ENV: String = format!("{}_LOGLEVEL", PROJECT_NAME.clone());
  pub static ref LOG_FILE: String = format!("{}.log", env!("CARGO_PKG_NAME"));
}

fn project_directory() -> Option<ProjectDirs> {
  ProjectDirs::from("com", "kdheepak", env!("CARGO_PKG_NAME"))
}

pub fn initialize_panic_handler() -> Result<()> {
  let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default()
    .panic_section(format!("This is a bug. Consider reporting it at {}", env!("CARGO_PKG_REPOSITORY")))
    .capture_span_trace_by_default(false)
    .display_location_section(false)
    .display_env_section(false)
    .into_hooks();
  eyre_hook.install()?;
  std::panic::set_hook(Box::new(move |panic_info| {
    if let Ok(mut t) = crate::tui::Tui::new() {
      if let Err(r) = t.exit() {
        error!("Unable to exit Terminal: {:?}", r);
      }
    }

    #[cfg(not(debug_assertions))]
    {
      use human_panic::{handle_dump, print_msg, Metadata};
      let meta = Metadata {
        version: env!("CARGO_PKG_VERSION").into(),
        name: env!("CARGO_PKG_NAME").into(),
        authors: env!("CARGO_PKG_AUTHORS").replace(':', ", ").into(),
        homepage: env!("CARGO_PKG_HOMEPAGE").into(),
      };

      let file_path = handle_dump(&meta, panic_info);
      // prints human-panic message
      print_msg(file_path, &meta).expect("human-panic: printing error message to console failed");
      eprintln!("{}", panic_hook.panic_report(panic_info)); // prints color-eyre stack trace to stderr
    }
    let msg = format!("{}", panic_hook.panic_report(panic_info));
    log::error!("Error: {}", strip_ansi_escapes::strip_str(msg));

    #[cfg(debug_assertions)]
    {
      // Better Panic stacktrace that is only enabled when debugging.
      better_panic::Settings::auto()
        .most_recent_first(false)
        .lineno_suffix(true)
        .verbosity(better_panic::Verbosity::Full)
        .create_panic_handler()(panic_info);
    }

    std::process::exit(libc::EXIT_FAILURE);
  }));
  Ok(())
}

pub fn get_data_dir() -> PathBuf {
  let directory = if let Some(s) = DATA_FOLDER.clone() {
    s
  } else if let Some(proj_dirs) = project_directory() {
    proj_dirs.data_local_dir().to_path_buf()
  } else {
    PathBuf::from(".").join(".data")
  };
  directory
}

pub fn get_config_dir() -> PathBuf {
  let directory = if let Some(s) = CONFIG_FOLDER.clone() {
    s
  } else if let Some(proj_dirs) = project_directory() {
    proj_dirs.config_local_dir().to_path_buf()
  } else {
    PathBuf::from(".").join(".config")
  };
  directory
}

pub fn initialize_logging() -> Result<()> {
  let log_file = File::create("app.log").expect("Failed to create log file");

  // 2. Configure the subscriber to write to the file.
  let subscriber = FmtSubscriber::builder()
        .with_writer(move || log_file.try_clone().unwrap()) // Clone for each log.
        .with_max_level(tracing::Level::DEBUG) // Set desired log level.
        .finish();

  // 3. Set the global subscriber.
  tracing::subscriber::set_global_default(subscriber).expect("Failed to set global subscriber");

  // 4. Use the logging macros.
  info!("Application started");
  Ok(())
}

/// Similar to the `std::dbg!` macro, but generates `tracing` events rather
/// than printing to stdout.
///
/// By default, the verbosity level for the generated events is `DEBUG`, but
/// this can be customized.
#[macro_export]
macro_rules! trace_dbg {
    (target: $target:expr, level: $level:expr, $ex:expr) => {{
        match $ex {
            value => {
                tracing::event!(target: $target, $level, ?value, stringify!($ex));
                value
            }
        }
    }};
    (level: $level:expr, $ex:expr) => {
        trace_dbg!(target: module_path!(), level: $level, $ex)
    };
    (target: $target:expr, $ex:expr) => {
        trace_dbg!(target: $target, level: tracing::Level::DEBUG, $ex)
    };
    ($ex:expr) => {
        trace_dbg!(level: tracing::Level::DEBUG, $ex)
    };
}

pub fn version() -> String {
  let author = clap::crate_authors!();

  let commit_hash = GIT_COMMIT_HASH.clone();

  // let current_exe_path = PathBuf::from(clap::crate_name!()).display().to_string();
  let config_dir_path = get_config_dir().display().to_string();
  let data_dir_path = get_data_dir().display().to_string();

  format!(
    "\
{commit_hash}

Authors: {author}

Config directory: {config_dir_path}
Data directory: {data_dir_path}"
  )
}
