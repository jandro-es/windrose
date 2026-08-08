//! The doctor tab: findings, and — only when asked — how to fix them.
//!
//! Windrose never runs an install command. This pane exists to explain what to
//! do and hand over the exact text, leaving the decision and the typing with
//! the person whose Mac it is.

use super::app::{App, DoctorMode};
use crate::doctor::{CheckResult, CheckStatus};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// Shown while a guide is open, so both available keys are always in view.
pub const FIX_FOOTER: &str = " c copy command · Esc back ";

pub fn draw(app: &App, frame: &mut Frame, area: Rect, block: Block) {
    match app.doctor.mode {
        DoctorMode::List => draw_list(app, frame, area, block),
        DoctorMode::FixDetail => draw_fix_detail(app, frame, area, block),
    }
}

fn draw_list(app: &App, frame: &mut Frame, area: Rect, block: Block) {
    let mut lines: Vec<Line> = app
        .doctor
        .results
        .iter()
        .enumerate()
        .map(|(i, check)| {
            let selected = i == app.doctor.selected;
            Line::from(vec![
                Span::raw(if selected { "› " } else { "  " }),
                Span::raw(format!("{} ", status_marker(check.status))),
                Span::styled(
                    check.title.clone(),
                    if selected {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
            ])
        })
        .collect();

    if let Some(check) = app.doctor.current() {
        lines.push(Line::from(""));
        lines.push(Line::from(check.explanation.clone()));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            if check.fix.is_some() {
                "Press Enter to see what to do about this."
            } else {
                "Nothing to do here."
            },
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

/// Human steps first, commands second.
///
/// The order is the point: someone who has never opened Terminal reads the
/// steps and follows them; someone who knows what they are doing skips to the
/// commands. Leading with a wall of shell would lose the first reader.
fn draw_fix_detail(app: &App, frame: &mut Frame, area: Rect, block: Block) {
    let Some(check) = app.doctor.current() else {
        return;
    };
    let Some(fix) = check.fix.as_ref() else {
        return;
    };

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Size the commands block to the wrapped text. A single `brew install` is
    // one line, but the Homebrew installer is a 90-character one-liner — and a
    // command silently cut off at the edge of the screen is one the reader
    // cannot check before running it.
    let width = usize::from(inner.width).saturating_sub(4).max(20);
    let command_lines: usize = fix
        .commands
        .iter()
        .map(|command| command.chars().count().div_ceil(width).max(1))
        .sum();

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),                           // steps
            Constraint::Length(command_lines as u16 + 2), // commands
            Constraint::Length(1),                        // footer
        ])
        .split(inner);

    draw_steps(check, fix, frame, split[0]);
    draw_commands(app, fix, frame, split[1]);

    frame.render_widget(
        Paragraph::new(FIX_FOOTER).style(Style::default().fg(Color::Black).bg(Color::Gray)),
        split[2],
    );
}

fn draw_steps(check: &CheckResult, fix: &crate::doctor::FixGuide, frame: &mut Frame, area: Rect) {
    let mut lines = vec![
        Line::from(Span::styled(
            check.title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(check.explanation.clone()),
        Line::from(""),
    ];

    for (n, step) in fix.steps.iter().enumerate() {
        lines.push(Line::from(format!("{}. {step}", n + 1)));
    }

    if let Some(url) = fix.docs_url {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("More detail: {url}"),
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::NONE))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_commands(app: &App, fix: &crate::doctor::FixGuide, frame: &mut Frame, area: Rect) {
    let title = if fix.commands.is_empty() {
        " No commands needed ".to_string()
    } else {
        " Commands — Windrose will not run these for you ".to_string()
    };

    let lines: Vec<Line> = fix
        .commands
        .iter()
        .enumerate()
        .map(|(i, command)| {
            let selected = i == app.doctor.command;
            Line::from(vec![
                Span::raw(if selected { "› " } else { "  " }),
                Span::styled(
                    command.clone(),
                    if selected {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Cyan)
                    },
                ),
            ])
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            // `trim: false` keeps the leading cursor column intact.
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn status_marker(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "✅",
        CheckStatus::Warn => "⚠️",
        CheckStatus::Fail => "❌",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::testing::MockSys;
    use crate::tui::app::{Msg, update};
    use crate::tui::view;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 44)).expect("test backend");
        terminal
            .draw(|frame| view::view(app, frame))
            .expect("drawing never fails");

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

    /// An app sitting on an open guide.
    fn with_open_guide() -> App {
        let mut app = App::with_fixture();
        let sys = MockSys::new();
        update(&mut app, key('5'), &sys);
        app.doctor.selected = app
            .doctor
            .results
            .iter()
            .position(|c| c.fix.as_ref().is_some_and(|f| !f.commands.is_empty()))
            .expect("the fixture should contain a guide with commands");
        update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &sys,
        );
        app
    }

    #[test]
    fn the_list_offers_a_guide_without_showing_it() {
        let mut app = App::with_fixture();
        update(&mut app, key('5'), &MockSys::new());
        let screen = render(&app);

        assert!(
            screen.contains("Press Enter"),
            "the guide should be offered"
        );
        assert!(
            !screen.contains("Open Terminal"),
            "no steps until they are asked for"
        );
    }

    /// Steps come before commands: a beginner reads down and follows along,
    /// and nobody is met by a wall of shell first.
    #[test]
    fn the_guide_shows_numbered_steps_above_the_commands() {
        let app = with_open_guide();
        let screen = render(&app);

        let first_step = screen.find("1. ").expect("numbered steps should be shown");
        let commands = screen
            .find("Commands")
            .expect("commands should be in their own block");
        assert!(
            first_step < commands,
            "steps must come before commands:\n{screen}"
        );
    }

    #[test]
    fn the_guide_shows_the_footer_with_both_keys() {
        let app = with_open_guide();
        let screen = render(&app);

        assert!(
            screen.contains("c copy command · Esc back"),
            "the footer should name both keys:\n{screen}"
        );
    }

    /// The commands block says out loud that Windrose will not run them, so
    /// nobody expects a button that does it for them.
    #[test]
    fn the_commands_block_says_windrose_will_not_run_them() {
        let app = with_open_guide();
        let screen = render(&app);

        assert!(screen.contains("will not run these for you"), "{screen}");
    }

    #[test]
    fn the_highlighted_command_is_marked() {
        let app = with_open_guide();
        let screen = render(&app);
        let command = app
            .doctor
            .current_command()
            .expect("a guide with commands")
            .to_string();

        // A long command wraps, so match its opening rather than the whole
        // string when looking for the cursor.
        let opening: String = command.chars().take(24).collect();
        let line = screen
            .lines()
            .find(|l| l.contains(&opening))
            .unwrap_or_else(|| panic!("command not on screen:\n{screen}"));
        assert!(
            line.contains('›'),
            "the copy target should be marked: {line}"
        );

        // And the whole command survived, rather than being cut off at the
        // edge — the reader has to be able to check what they are copying.
        // Compared with the frame's decoration and spacing removed.
        let strip = |text: &str| -> String {
            text.chars()
                .filter(|c| !c.is_whitespace() && !"│┌┐└┘─›".contains(*c))
                .collect()
        };
        assert!(
            strip(&screen).contains(&strip(&command)),
            "the command was cut off instead of wrapped:\n{screen}"
        );
    }

    /// Leaving the guide returns to the findings, not to a blank pane.
    #[test]
    fn going_back_shows_the_list_again() {
        let mut app = with_open_guide();
        update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            &MockSys::new(),
        );

        let screen = render(&app);
        assert!(screen.contains("Press Enter"));
        assert!(!screen.contains("c copy command"));
    }
}
