//! Drawing. Reads [`App`], writes to a frame, changes nothing.

use super::app::{App, Tab};
use crate::model::{Availability, Detection};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap};

/// Always on screen, so nobody has to guess how to get out. The spec requires
/// this to be visible at all times, not hidden behind the help key.
pub const HELP_BAR: &str = " ↑↓ move · Tab switch · ? help · q quit ";

pub const TITLE: &str = "Windrose";

pub fn view(app: &App, frame: &mut Frame) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(1),    // body
            Constraint::Length(1), // help bar
        ])
        .split(frame.area());

    draw_tabs(app, frame, areas[0]);
    draw_body(app, frame, areas[1]);
    draw_help_bar(frame, areas[2]);

    if app.show_help {
        draw_help_overlay(frame, frame.area());
    }
}

/// The splash shown while the scan runs, before there is anything to show.
pub fn draw_scanning(frame: &mut Frame) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {TITLE} "));
    let text = vec![
        Line::from(""),
        Line::from("Scanning your Mac… (a few seconds)"),
        Line::from(""),
        Line::from(Span::styled(
            "Looking for AI tools, checking what is running, and measuring what this Mac can do.",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(
        Paragraph::new(text)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        frame.area(),
    );
}

fn draw_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            Line::from(vec![
                Span::styled(format!("{} ", i + 1), Style::default().fg(Color::DarkGray)),
                Span::raw(tab.title()),
            ])
        })
        .collect();

    let selected = Tab::ALL
        .iter()
        .position(|t| *t == app.tab)
        .unwrap_or_default();

    frame.render_widget(
        Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {TITLE} ")),
            )
            .select(selected)
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        area,
    );
}

fn draw_help_bar(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(HELP_BAR).style(Style::default().fg(Color::Black).bg(Color::Gray)),
        area,
    );
}

/// Per-tab body. Task 13 replaces these with the real content views.
fn draw_body(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", app.tab.title()));

    match app.tab {
        Tab::Overview => {
            let hw = &app.scan.hardware;
            let text = vec![
                Line::from(format!(
                    "{} · {} GB memory · macOS {}",
                    hw.chip_name, hw.ram_gb, hw.macos_major
                )),
                Line::from(""),
                Line::from(format!(
                    "Overall score {}/100 — memory {}/100, chip {}/100",
                    app.scan.score.overall, app.scan.score.memory, app.scan.score.compute
                )),
                Line::from(""),
                Line::from(format!(
                    "{} AI options found, {} ready to use.",
                    app.scan.detections.len(),
                    ready_count(app)
                )),
            ];
            frame.render_widget(Paragraph::new(text).block(block), area);
        }
        Tab::Local => draw_detection_list(app, frame, area, block, app.local_detections()),
        Tab::Cloud => draw_detection_list(app, frame, area, block, app.cloud_detections()),
        Tab::Score => {
            let lines: Vec<Line> = app
                .scan
                .score
                .tiers
                .iter()
                .map(|tier| Line::from(format!("{:<16} {}", tier.label, tier.advice)))
                .collect();
            frame.render_widget(Paragraph::new(lines).block(block), area);
        }
        Tab::Doctor => draw_doctor(app, frame, area, block),
    }
}

fn draw_detection_list<'a>(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    block: Block,
    detections: impl Iterator<Item = &'a Detection>,
) {
    let items: Vec<ListItem> = detections
        .enumerate()
        .map(|(i, det)| {
            ListItem::new(Line::from(format!(
                "{} {} {} — {}",
                cursor(i == app.selected),
                marker(&det.availability),
                det.name,
                state_words(&det.availability)
            )))
        })
        .collect();

    frame.render_widget(List::new(items).block(block), area);
}

fn draw_doctor(app: &App, frame: &mut Frame, area: Rect, block: Block) {
    let checks: Vec<_> = app.scan.health.iter().chain(&app.scan.perf).collect();

    let mut lines: Vec<Line> = checks
        .iter()
        .enumerate()
        .map(|(i, check)| {
            Line::from(format!(
                "{} {} {}",
                cursor(i == app.selected),
                status_marker(check.status),
                check.title
            ))
        })
        .collect();

    // Setup guidance is opt-in, so this offers rather than shows.
    match checks.get(app.selected) {
        Some(check) if check.fix.is_some() && app.doctor.showing_fix => {
            let fix = check.fix.as_ref().expect("checked just above");
            lines.push(Line::from(""));
            for (n, step) in fix.steps.iter().enumerate() {
                lines.push(Line::from(format!("  {}. {step}", n + 1)));
            }
            for command in &fix.commands {
                lines.push(Line::from(Span::styled(
                    format!("    {command}"),
                    Style::default().fg(Color::Cyan),
                )));
            }
        }
        Some(check) if check.fix.is_some() => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Press Enter to see the setup steps for this item.",
                Style::default().fg(Color::DarkGray),
            )));
        }
        _ => {}
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    let popup = centred(60, 50, area);
    frame.render_widget(Clear, popup);

    let text = vec![
        Line::from("Moving around"),
        Line::from("  1–5          jump straight to a tab"),
        Line::from("  Tab / ⇧Tab   next or previous tab"),
        Line::from("  ↑ ↓ or j k   move through the list"),
        Line::from(""),
        Line::from("On the Doctor tab"),
        Line::from("  Enter        show the setup steps for the selected item"),
        Line::from(""),
        Line::from("Leaving"),
        Line::from("  q or Esc     quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close this.",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Keys ")
                .style(Style::default().bg(Color::Black)),
        ),
        popup,
    );
}

/// A box of the given percentage size, centred in `area`.
fn centred(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn ready_count(app: &App) -> usize {
    app.scan
        .detections
        .iter()
        .filter(|d| d.availability == Availability::Ready)
        .count()
}

fn cursor(selected: bool) -> &'static str {
    if selected { "›" } else { " " }
}

fn marker(availability: &Availability) -> &'static str {
    match availability {
        Availability::Ready => "✅",
        Availability::InstalledNotRunning | Availability::Partial(_) => "⚠️",
        Availability::NotFound => "❌",
    }
}

fn state_words(availability: &Availability) -> String {
    match availability {
        Availability::Ready => "ready to use".to_string(),
        Availability::InstalledNotRunning => "installed, but not running".to_string(),
        Availability::Partial(reason) => format!("half set up — {reason}"),
        Availability::NotFound => "not installed".to_string(),
    }
}

fn status_marker(status: crate::doctor::CheckStatus) -> &'static str {
    use crate::doctor::CheckStatus;
    match status {
        CheckStatus::Pass => "✅",
        CheckStatus::Warn => "⚠️",
        CheckStatus::Fail => "❌",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{Msg, update};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// Render one frame and flatten the buffer into searchable text.
    fn render(app: &App) -> String {
        let mut terminal =
            Terminal::new(TestBackend::new(100, 40)).expect("test backend never fails");
        terminal
            .draw(|frame| view(app, frame))
            .expect("drawing to a test backend never fails");

        let buffer = terminal.backend().buffer().clone();
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .chunks(buffer.area.width as usize)
            .map(|row| row.concat())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn key(c: char) -> Msg {
        Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    #[test]
    fn renders_title_every_tab_name_and_the_help_bar() {
        let screen = render(&App::with_fixture());

        assert!(screen.contains(TITLE), "no app title");
        for tab in Tab::ALL {
            assert!(screen.contains(tab.title()), "missing tab: {}", tab.title());
        }
        assert!(
            screen.contains("↑↓ move · Tab switch · ? help · q quit"),
            "the persistent help bar must always be visible"
        );
    }

    /// The help bar is the way out; it must survive every tab and the overlay.
    #[test]
    fn the_help_bar_is_present_on_every_tab() {
        let mut app = App::with_fixture();

        for n in "12345".chars() {
            update(&mut app, key(n));
            assert!(
                render(&app).contains("q quit"),
                "help bar missing on {}",
                app.tab.title()
            );
        }
    }

    #[test]
    fn the_help_overlay_appears_and_lists_the_keys() {
        let mut app = App::with_fixture();
        assert!(!render(&app).contains("Press any key to close"));

        update(&mut app, key('?'));
        let screen = render(&app);
        assert!(screen.contains("Press any key to close"));
        assert!(screen.contains("quit"));
    }

    #[test]
    fn the_local_tab_lists_options_with_a_cursor() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'));
        let screen = render(&app);

        assert!(screen.contains("Ollama"));
        assert!(screen.contains("›"), "no selection cursor");
        assert!(
            screen.contains("ready to use"),
            "states need words, not just markers"
        );
    }

    /// The splash has to say what is happening and roughly how long it takes.
    #[test]
    fn the_scanning_splash_explains_the_wait() {
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).expect("test backend");
        terminal.draw(draw_scanning).expect("drawing never fails");

        let buffer = terminal.backend().buffer().clone();
        let screen: String = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .concat();

        assert!(screen.contains("Scanning your Mac"));
        assert!(screen.contains("few seconds"));
    }

    /// Setup steps must not be on screen until the user asks for them.
    #[test]
    fn the_doctor_tab_offers_steps_before_showing_them() {
        let mut app = App::with_fixture();
        update(&mut app, key('5'));

        // Every guide opens by saying how to reach a Terminal, so its absence
        // is a reliable sign that no steps are on screen — unlike a specific
        // command, which differs from one finding to the next.
        let offered = render(&app);
        assert!(offered.contains("Press Enter"));
        assert!(
            !offered.contains("Open Terminal"),
            "setup steps must not appear until they are asked for"
        );

        update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );
        let shown = render(&app);
        assert!(shown.contains("Open Terminal"), "steps should now be shown");
        assert!(shown.contains("1."), "steps should be numbered");
    }
}
