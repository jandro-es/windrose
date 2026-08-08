//! Drawing. Reads [`App`], writes to a frame, changes nothing.

use super::app::{App, Tab};
use super::doctor_view;
use super::help;
use crate::model::{Availability, Detection};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Table, Tabs, Wrap};

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

    if app.show_detail {
        draw_detail_popup(app, frame, frame.area());
    }
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

/// Per-tab body.
fn draw_body(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", app.tab.title()));

    match app.tab {
        Tab::Overview => draw_overview(app, frame, area, block),
        Tab::Local => draw_detections(app, frame, area, block, app.local_detections()),
        Tab::Cloud => draw_detections(app, frame, area, block, app.cloud_detections()),
        Tab::Score => draw_score(app, frame, area, block),
        Tab::Doctor => doctor_view::draw(app, frame, area, block),
    }
}

/// Counts of each state, in words rather than only symbols.
struct Counts {
    ready: usize,
    attention: usize,
    missing: usize,
}

fn counts(app: &App) -> Counts {
    let mut counts = Counts {
        ready: 0,
        attention: 0,
        missing: 0,
    };
    for det in &app.scan.detections {
        match det.availability {
            Availability::Ready => counts.ready += 1,
            Availability::InstalledNotRunning | Availability::Partial(_) => counts.attention += 1,
            Availability::NotFound => counts.missing += 1,
        }
    }
    counts
}

fn draw_overview(app: &App, frame: &mut Frame, area: Rect, block: Block) {
    let hw = &app.scan.hardware;
    let c = counts(app);
    let shape = if hw.is_laptop { "laptop" } else { "desktop" };

    let text = vec![
        Line::from(Span::styled(
            format!(
                "{} · {} GB memory · macOS {} · {shape}",
                hw.chip_name, hw.ram_gb, hw.macos_major
            ),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "{} ready · {} need attention · {} not installed",
            c.ready, c.attention, c.missing
        )),
        Line::from(""),
        Line::from(format!(
            "This Mac scores {}/100 for running models on its own.",
            app.scan.score.overall
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press 2 for what is on this Mac, 3 for cloud services, 4 for the score, \
             5 for things worth fixing.",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

/// A table of options, with the selected row explained underneath.
fn draw_detections<'a>(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    block: Block,
    detections: impl Iterator<Item = &'a Detection>,
) {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(5)])
        .split(area);

    let rows: Vec<Row> = detections
        .enumerate()
        .map(|(i, det)| {
            let style = if i == app.selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(cursor(i == app.selected)),
                Cell::from(marker(&det.availability)),
                Cell::from(det.name),
                Cell::from(det.version.clone().unwrap_or_else(|| "—".to_string())),
                Cell::from(state_words(&det.availability)),
            ])
            .style(style)
        })
        .collect();

    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Length(24),
                Constraint::Length(12),
                Constraint::Min(20),
            ],
        )
        .header(
            Row::new(vec!["", "", "Option", "Version", "Status"])
                .style(Style::default().fg(Color::DarkGray)),
        )
        .block(block),
        split[0],
    );

    draw_detail_pane(app, frame, split[1]);
}

/// The plain-English explanation of whatever the cursor is on. This is the
/// plain-language rule made visible: the answer to "what even is this?" is
/// always on screen, not hidden behind a key.
fn draw_detail_pane(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" What is this? ");

    let text = match app.selected_detection() {
        Some(det) => vec![
            Line::from(det.friendly.clone()),
            Line::from(""),
            Line::from(Span::styled(
                "Press Enter for everything Windrose found about it.",
                Style::default().fg(Color::DarkGray),
            )),
        ],
        None => vec![Line::from("Nothing to show.")],
    };

    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_score(app: &App, frame: &mut Frame, area: Rect, block: Block) {
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(inner);

    let score = &app.scan.score;
    frame.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::ALL).title(" Overall "))
            .gauge_style(Style::default().fg(Color::Cyan))
            .percent(score.overall.min(100) as u16)
            .label(format!(
                "{}/100 — memory {}/100, chip {}/100",
                score.overall, score.memory, score.compute
            )),
        split[0],
    );

    let rows: Vec<Row> = score
        .tiers
        .iter()
        .map(|tier| {
            Row::new(vec![
                Cell::from(tier.label),
                Cell::from(fit_words(tier.fits)),
                Cell::from(speed_words(tier)),
                Cell::from(tier.advice.clone()),
            ])
        })
        .collect();

    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(16),
                Constraint::Length(10),
                Constraint::Length(18),
                Constraint::Min(20),
            ],
        )
        .header(
            Row::new(vec!["Model size", "Fits", "Rough speed", "What that means"])
                .style(Style::default().fg(Color::DarkGray)),
        ),
        split[1],
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            crate::scoring::QUANTISATION_NOTE,
            Style::default().fg(Color::DarkGray),
        ))
        .wrap(Wrap { trim: true }),
        split[2],
    );
}

/// Everything Windrose found about one option, plus what it actually is.
fn draw_detail_popup(app: &App, frame: &mut Frame, area: Rect) {
    let Some(det) = app.selected_detection() else {
        return;
    };

    let popup = centred(70, 60, area);
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::from(Span::styled(
            "What is this?",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(det.friendly.clone()),
        Line::from(""),
        Line::from(Span::styled(
            "What Windrose found",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  Status: {}", state_words(&det.availability))),
    ];

    if let Some(version) = &det.version {
        lines.push(Line::from(format!("  Version: {version}")));
    }
    // Detail rows are booleans and paths by construction — the secrets rule
    // means a credential's value never reaches a Detection in the first place.
    for (key, value) in &det.details {
        lines.push(Line::from(format!("  {key}: {value}")));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press any key to close this.",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", det.name))
                    .style(Style::default().bg(Color::Black)),
            )
            .wrap(Wrap { trim: true }),
        popup,
    );
}

fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    // Nearly the whole screen: the glossary is long, and a cramped box would
    // hide most of it.
    let popup = centred(92, 92, area);
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::from(Span::styled(
            "Keys",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  1–5          jump straight to a tab"),
        Line::from("  Tab / ⇧Tab   next or previous tab"),
        Line::from("  ↑ ↓ or j k   move through the list"),
        Line::from("  Enter        show more about the selected row"),
        Line::from("  q or Esc     quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Words you may meet",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    ];

    for (term, explanation) in help::glossary() {
        lines.push(Line::from(vec![
            Span::styled(format!("  {term}: "), Style::default().fg(Color::Cyan)),
            Span::raw(explanation),
        ]));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    // The way out lives in the title, not the last line: a
                    // glossary longer than the terminal would push a closing
                    // hint off the bottom, stranding the reader.
                    .title(" Help — press any key to close ")
                    .style(Style::default().bg(Color::Black)),
            )
            .wrap(Wrap { trim: true }),
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

fn fit_words(fits: crate::scoring::Fit) -> &'static str {
    use crate::scoring::Fit;
    match fits {
        Fit::Great => "Great",
        Fit::Ok => "OK",
        Fit::Tight => "Tight",
        Fit::No => "Won't fit",
    }
}

fn speed_words(tier: &crate::scoring::ModelTierFit) -> String {
    match tier.est_tok_s {
        Some((low, high)) => format!("{low}–{high} words/sec"),
        None => "—".to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::testing::MockSys;
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

    fn key_code(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
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
            update(&mut app, key(n), &MockSys::new());
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
        assert!(!render(&app).contains("press any key to close"));

        update(&mut app, key('?'), &MockSys::new());
        let screen = render(&app);
        // The way out is in the title, so a long glossary cannot hide it.
        assert!(screen.contains("press any key to close"));
        assert!(screen.contains("quit"));
    }

    #[test]
    fn the_local_tab_lists_options_with_a_cursor() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());
        let screen = render(&app);

        assert!(screen.contains("Ollama"));
        assert!(screen.contains("›"), "no selection cursor");
        assert!(
            screen.contains("ready to use"),
            "states need words, not just markers"
        );
    }

    #[test]
    fn the_overview_shows_counts_and_the_hardware_summary() {
        let app = App::with_fixture();
        let screen = render(&app);

        // The fixture has 3 ready, 1 needing attention, 2 not installed.
        assert!(
            screen.contains("3 ready · 1 need attention · 2 not installed"),
            "counts missing or wrong:\n{screen}"
        );
        assert!(screen.contains("Apple M4 Pro"), "no chip name");
        assert!(screen.contains("48 GB memory"), "no memory");
        assert!(screen.contains("macOS 26"), "no macOS version");
    }

    /// Counts must add up to the number of options found, or the Overview is
    /// quietly lying about what the other tabs contain.
    #[test]
    fn the_overview_counts_account_for_every_option() {
        let app = App::with_fixture();
        let c = counts(&app);

        assert_eq!(c.ready + c.attention + c.missing, app.scan.detections.len());
    }

    /// The plain-language rule: the answer to "what is this?" is on screen for
    /// whatever the cursor is on, without pressing anything.
    #[test]
    fn the_local_tab_explains_the_selected_row_in_a_detail_pane() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());

        let screen = render(&app);
        assert!(screen.contains("What is this?"), "no detail pane");
        assert!(
            screen.contains("a plain-English explanation"),
            "the friendly line of the selected row should be shown"
        );
    }

    /// The pane follows the cursor rather than showing a fixed row.
    #[test]
    fn the_detail_pane_follows_the_selection() {
        let mut app = App::with_fixture();
        update(&mut app, key('3'), &MockSys::new());
        assert!(render(&app).contains("Claude"));

        update(&mut app, key_code(KeyCode::Down), &MockSys::new());
        assert!(render(&app).contains("Groq"));
    }

    #[test]
    fn the_local_tab_is_a_table_with_headings() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());
        let screen = render(&app);

        for heading in ["Option", "Version", "Status"] {
            assert!(screen.contains(heading), "missing column: {heading}");
        }
        assert!(screen.contains("Ollama"));
        assert!(screen.contains("LM Studio"));
    }

    #[test]
    fn the_score_tab_shows_a_gauge_and_every_tier_with_its_advice() {
        let mut app = App::with_fixture();
        update(&mut app, key('4'), &MockSys::new());
        let screen = render(&app);

        assert!(
            screen.contains(&format!("{}/100", app.scan.score.overall)),
            "no overall score"
        );
        for tier in &app.scan.score.tiers {
            assert!(screen.contains(tier.label), "missing tier: {}", tier.label);
            assert!(
                screen.contains(tier.advice.split(" — ").next().unwrap_or(&tier.advice)),
                "tier {} is missing its advice",
                tier.label
            );
        }
    }

    /// The jargon in the tier labels is explained on the same screen.
    #[test]
    fn the_score_tab_explains_its_own_jargon() {
        let mut app = App::with_fixture();
        update(&mut app, key('4'), &MockSys::new());

        assert!(render(&app).contains("compressed"));
    }

    #[test]
    fn enter_opens_a_detail_popup_with_every_row_windrose_found() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());
        update(&mut app, key_code(KeyCode::Enter), &MockSys::new());
        let screen = render(&app);

        assert!(
            screen.contains("Ollama"),
            "the popup should name the option"
        );
        assert!(
            screen.contains("Example row"),
            "detail rows should be listed"
        );
        assert!(screen.contains("What is this?"), "and what it actually is");
    }

    /// The help overlay carries the glossary, which is where the plain-language
    /// rule sends anyone who meets a word they do not know.
    #[test]
    fn the_help_overlay_carries_the_glossary() {
        let mut app = App::with_fixture();
        update(&mut app, key('?'), &MockSys::new());
        let screen = render(&app);

        for term in ["model", "token", "API key"] {
            assert!(screen.contains(term), "glossary missing: {term}");
        }
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
        update(&mut app, key('5'), &MockSys::new());

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
            &MockSys::new(),
        );
        let shown = render(&app);
        assert!(shown.contains("Open Terminal"), "steps should now be shown");
        assert!(shown.contains("1."), "steps should be numbered");
    }
}
