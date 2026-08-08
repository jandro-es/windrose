//! The interactive dashboard.
//!
//! This module owns the terminal, and with it one hard rule: **whatever
//! happens, the terminal is put back the way it was found.** Raw mode and the
//! alternate screen are global state belonging to the user's shell, not to us.
//! A normal exit, an error, or a panic anywhere in the drawing code must all
//! leave a working prompt behind.

mod app;
mod view;

use app::{Action, App, Msg, update};
use crossterm::event::{self, Event};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, terminal};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{Stdout, stdout};
use std::sync::mpsc;
use std::time::Duration;

/// How long to wait for a key before drawing another frame.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Open the dashboard.
pub fn run() -> Result<(), String> {
    install_panic_hook();

    let mut terminal = setup().map_err(|e| format!("could not start the dashboard: {e}"))?;
    let outcome = event_loop(&mut terminal);

    // Restore before reporting anything: an error message is no use printed
    // into a terminal still in raw mode on an alternate screen.
    let restored = restore();
    outcome.and(restored)
}

fn setup() -> std::io::Result<Tui> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

/// Undo everything [`setup`] did. Safe to call more than once, and safe to call
/// when setup only half succeeded — each step is independent and ignoring a
/// failure here is better than skipping the steps that follow it.
fn restore() -> Result<(), String> {
    let raw = terminal::disable_raw_mode();
    let screen = execute!(stdout(), LeaveAlternateScreen);

    raw.and(screen)
        .map_err(|e| format!("could not restore the terminal: {e}"))
}

/// Put the terminal back before the panic message is printed.
///
/// Without this, a panic inside the drawing code leaves the user staring at a
/// dead alternate screen with no echo and no working Ctrl-C — the panic text
/// scrolls past invisibly and the shell appears to have hung.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        previous(info);
    }));
}

fn event_loop(terminal: &mut Tui) -> Result<(), String> {
    let scan = scan_with_splash(terminal)?;
    let mut app = App::new(scan);

    loop {
        terminal
            .draw(|frame| view::view(&app, frame))
            .map_err(|e| format!("could not draw the dashboard: {e}"))?;

        let msg = match next_event()? {
            Some(msg) => msg,
            None => continue,
        };

        if update(&mut app, msg) == Action::Quit {
            return Ok(());
        }
    }
}

/// Wait briefly for input, returning `None` when nothing happened.
fn next_event() -> Result<Option<Msg>, String> {
    if !event::poll(POLL_INTERVAL).map_err(|e| format!("could not read input: {e}"))? {
        return Ok(Some(Msg::Tick));
    }

    match event::read().map_err(|e| format!("could not read input: {e}"))? {
        // Windows sends both press and release; reacting to both would move
        // two rows for every key. Crossterm reports the kind, so filter here.
        Event::Key(key) if key.kind == event::KeyEventKind::Press => Ok(Some(Msg::Key(key))),
        _ => Ok(None),
    }
}

/// Did the user press a quit key while waiting? Never blocks.
fn cancelled() -> bool {
    if !event::poll(Duration::ZERO).unwrap_or(false) {
        return false;
    }
    match event::read() {
        Ok(Event::Key(key)) => matches!(key.code, event::KeyCode::Char('q') | event::KeyCode::Esc),
        _ => false,
    }
}

/// Run the scan on another thread, drawing a splash until it finishes.
///
/// A scan takes a couple of seconds — long enough that a frozen blank screen
/// would look broken.
fn scan_with_splash(terminal: &mut Tui) -> Result<crate::report::ScanResult, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::report::gather(&crate::sys::RealSys));
    });

    loop {
        terminal
            .draw(view::draw_scanning)
            .map_err(|e| format!("could not draw the dashboard: {e}"))?;

        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(scan) => return Ok(scan),
            // Let the user give up on a scan that is taking too long.
            Err(mpsc::RecvTimeoutError::Timeout) if cancelled() => {
                return Err("Cancelled.".to_string());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("the scan stopped unexpectedly".to_string());
            }
        }
    }
}
