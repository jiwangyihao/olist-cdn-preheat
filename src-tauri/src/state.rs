use crate::models::{RunSnapshot, SiteSettings, RunSettings};
use std::sync::Mutex;

#[allow(dead_code)]
pub struct AppState {
    pub current_run: Mutex<Option<RunSnapshot>>,
    pub site_settings: Mutex<Option<SiteSettings>>,
    pub run_settings: Mutex<RunSettings>,
    // We might need a handle to the running task to cancel it
    pub cancel_token: Mutex<Option<tokio::sync::mpsc::Sender<()>>>,
    // Channel to send files to the running task
    pub run_sender: Mutex<Option<tokio::sync::mpsc::UnboundedSender<crate::models::FileItem>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_run: Mutex::new(None),
            site_settings: Mutex::new(None),
            run_settings: Mutex::new(RunSettings::default()),
            cancel_token: Mutex::new(None),
            run_sender: Mutex::new(None),
        }
    }
}
