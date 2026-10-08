use std::sync::Mutex;
use std::time::{Duration, Instant};

use esteban_core::progress::Progress;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const EVENT: &str = "install-progress";

const EVERY: Duration = Duration::from_millis(80);

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub stage: String,
    pub files: usize,
    pub files_done: usize,
    pub bytes: u64,
    pub bytes_done: u64,
    pub notice: Option<String>,
    pub done: bool,
}

pub struct UiProgress {
    app: AppHandle,
    state: Mutex<(Snapshot, Option<Instant>)>,
}

impl UiProgress {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            state: Mutex::new((Snapshot::default(), None)),
        }
    }

    fn update(&self, force: bool, change: impl FnOnce(&mut Snapshot)) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let (snapshot, last) = &mut *state;
        change(snapshot);
        let now = Instant::now();
        if !force && last.is_some_and(|at| now.duration_since(at) < EVERY) {
            return;
        }
        let _previous = last.replace(now);
        let payload = snapshot.clone();
        snapshot.notice = None;
        drop(state);
        if self.app.emit(EVENT, payload).is_err() {
            eprintln!("could not send install progress to the window");
        }
    }

    pub fn finish(&self) {
        self.update(true, |s| s.done = true);
    }
}

impl Progress for UiProgress {
    fn stage(&self, name: &str, files: usize, bytes: u64) {
        self.update(true, |s| {
            s.stage = name.to_string();
            s.files = files;
            s.files_done = 0;
            s.bytes = bytes;
            s.bytes_done = 0;
        });
    }

    fn advance(&self, bytes: u64) {
        self.update(false, |s| s.bytes_done = s.bytes_done.saturating_add(bytes));
    }

    fn file_done(&self) {
        self.update(false, |s| s.files_done = s.files_done.saturating_add(1));
    }

    fn notice(&self, message: &str) {
        self.update(true, |s| s.notice = Some(message.to_string()));
    }
}
