use std::io::{IsTerminal, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use esteban_core::progress::Progress;

struct State {
    stage: String,
    files: usize,
    files_done: usize,
    bytes: u64,
    bytes_done: u64,
    last_draw: Instant,
    open_line: bool,
}

pub struct CliProgress {
    state: Mutex<State>,
    tty: bool,
}

impl CliProgress {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                stage: String::new(),
                files: 0,
                files_done: 0,
                bytes: 0,
                bytes_done: 0,
                last_draw: Instant::now(),
                open_line: false,
            }),
            tty: std::io::stderr().is_terminal(),
        }
    }

    pub fn finish(&self) {
        if let Ok(mut state) = self.state.lock() {
            Self::close(&mut state, self.tty);
        }
    }

    fn line(state: &State) -> String {
        let mb = |b: u64| b as f64 / 1_048_576.0;
        format!(
            "  {:<14} {:>5}/{:<5} files  {:>7.1}/{:.1} MB",
            state.stage,
            state.files_done,
            state.files,
            mb(state.bytes_done),
            mb(state.bytes)
        )
    }

    fn draw(state: &mut State, tty: bool, force: bool) {
        if !force && state.last_draw.elapsed() < Duration::from_millis(if tty { 120 } else { 3000 })
        {
            return;
        }
        state.last_draw = Instant::now();
        let mut err = std::io::stderr().lock();
        if tty {
            let _ = write!(err, "\r{}", Self::line(state));
            state.open_line = true;
        } else {
            let _ = writeln!(err, "{}", Self::line(state));
        }
    }

    fn close(state: &mut State, tty: bool) {
        if state.stage.is_empty() {
            return;
        }
        state.bytes_done = state.bytes_done.max(state.bytes);
        Self::draw(state, tty, true);
        if tty && state.open_line {
            eprintln!();
            state.open_line = false;
        }
        state.stage.clear();
    }
}

impl Progress for CliProgress {
    fn stage(&self, name: &str, files: usize, bytes: u64) {
        if let Ok(mut state) = self.state.lock() {
            Self::close(&mut state, self.tty);
            state.stage = name.to_string();
            state.files = files;
            state.files_done = 0;
            state.bytes = bytes;
            state.bytes_done = 0;
            Self::draw(&mut state, self.tty, true);
        }
    }

    fn advance(&self, bytes: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.bytes_done += bytes;
            Self::draw(&mut state, self.tty, false);
        }
    }

    fn file_done(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.files_done += 1;
            Self::draw(&mut state, self.tty, false);
        }
    }

    fn notice(&self, message: &str) {
        if let Ok(mut state) = self.state.lock() {
            if self.tty && state.open_line {
                eprintln!();
                state.open_line = false;
            }
            eprintln!("{message}");
        }
    }
}
