//! The state and the state machine.
//!
//! Elm architecture: all state lives in [`App`], every input becomes a [`Msg`],
//! and [`update`] is the only thing that changes state. It touches no terminal
//! and no operating system, so the whole interaction model is testable without
//! drawing anything.

use crate::doctor::CheckResult;
use crate::model::Category;
use crate::report::ScanResult;
use crate::sys::SysCtx;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Local,
    Cloud,
    Score,
    Doctor,
}

impl Tab {
    /// Tabs in display order. The number keys follow this order.
    pub const ALL: [Tab; 5] = [
        Tab::Overview,
        Tab::Local,
        Tab::Cloud,
        Tab::Score,
        Tab::Doctor,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Local => "On this Mac",
            Tab::Cloud => "Cloud",
            Tab::Score => "Score",
            Tab::Doctor => "Doctor",
        }
    }

    fn index(self) -> usize {
        Tab::ALL
            .iter()
            .position(|t| *t == self)
            .expect("every tab is in ALL")
    }

    fn at(index: usize) -> Tab {
        Tab::ALL[index % Tab::ALL.len()]
    }
}

/// What the doctor tab is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoctorMode {
    /// The findings. Setup guidance is opt-in, so this is where you start.
    #[default]
    List,
    /// The steps and commands for one finding, because the user asked.
    FixDetail,
}

/// Doctor-tab state.
#[derive(Debug, Clone, Default)]
pub struct DoctorState {
    /// Health findings followed by performance findings, in report order.
    pub results: Vec<CheckResult>,
    pub selected: usize,
    pub mode: DoctorMode,
    /// Which command in the open guide is highlighted for copying.
    pub command: usize,
}

impl DoctorState {
    pub fn current(&self) -> Option<&CheckResult> {
        self.results.get(self.selected)
    }

    /// The command the user would copy, when there is one.
    pub fn current_command(&self) -> Option<&str> {
        let fix = self.current()?.fix.as_ref()?;
        fix.commands.get(self.command).map(String::as_str)
    }
}

pub struct App {
    pub scan: ScanResult,
    pub tab: Tab,
    pub selected: usize,
    pub show_help: bool,
    /// Whether the detail popup for the selected option is open.
    pub show_detail: bool,
    pub doctor: DoctorState,
}

/// What the event loop should do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Continue,
    Quit,
}

#[derive(Debug, Clone, Copy)]
pub enum Msg {
    Key(KeyEvent),
    /// The clock, for anything that needs to animate. Changes no state today.
    Tick,
}

impl App {
    pub fn new(scan: ScanResult) -> Self {
        let results = scan
            .health
            .iter()
            .chain(&scan.perf)
            .cloned()
            .collect::<Vec<_>>();

        Self {
            doctor: DoctorState {
                results,
                ..DoctorState::default()
            },
            scan,
            tab: Tab::Overview,
            selected: 0,
            show_help: false,
            show_detail: false,
        }
    }

    /// The option the cursor is on, when the current tab lists options.
    pub fn selected_detection(&self) -> Option<&crate::model::Detection> {
        match self.tab {
            Tab::Local => self.local_detections().nth(self.selected),
            Tab::Cloud => self.cloud_detections().nth(self.selected),
            _ => None,
        }
    }

    /// How many rows the current tab can move through.
    pub fn item_count(&self) -> usize {
        match self.tab {
            Tab::Overview | Tab::Score => 0,
            Tab::Local => self.local_detections().count(),
            Tab::Cloud => self.cloud_detections().count(),
            Tab::Doctor => self.doctor.results.len(),
        }
    }

    pub fn local_detections(&self) -> impl Iterator<Item = &crate::model::Detection> {
        self.scan.detections.iter().filter(|d| {
            matches!(
                d.category,
                Category::LocalRuntime | Category::OptimisedRuntime | Category::ApplePlatform
            )
        })
    }

    pub fn cloud_detections(&self) -> impl Iterator<Item = &crate::model::Detection> {
        self.scan
            .detections
            .iter()
            .filter(|d| d.category == Category::CloudProvider)
    }
}

/// The whole interaction model.
///
/// **Deviation from the plan's signature:** this takes `sys` because copying a
/// command to the clipboard is a real effect on the machine, and everything
/// that touches the machine goes through `SysCtx`. Keeping it here is what lets
/// a test assert that Windrose copied a command rather than running it.
pub fn update(app: &mut App, msg: Msg, sys: &dyn SysCtx) -> Action {
    let Msg::Key(key) = msg else {
        return Action::Continue;
    };

    // Overlays swallow input: any key closes them, so a reader cannot get
    // stuck behind one wondering which key dismisses it.
    if app.show_help {
        app.show_help = false;
        return Action::Continue;
    }
    if app.show_detail {
        app.show_detail = false;
        return Action::Continue;
    }

    // The fix pane is a place you are *inside*, so its keys are handled before
    // the global ones: Esc means "back to the list" here, not "quit".
    if app.tab == Tab::Doctor && app.doctor.mode == DoctorMode::FixDetail {
        return update_fix_detail(app, key, sys);
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
        // Ctrl-C is muscle memory for "stop"; honouring it avoids the user
        // reaching for a harsher way out that skips the terminal restore.
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Action::Quit;
        }
        KeyCode::Char('?') => app.show_help = true,

        KeyCode::Char(c @ '1'..='5') => {
            let index = c as usize - '1' as usize;
            select_tab(app, Tab::at(index));
        }
        KeyCode::Tab | KeyCode::Right => select_tab(app, Tab::at(app.tab.index() + 1)),
        KeyCode::BackTab | KeyCode::Left => {
            let last = Tab::ALL.len() - 1;
            select_tab(app, Tab::at(app.tab.index() + last));
        }

        KeyCode::Down | KeyCode::Char('j') => move_selection(app, 1),
        KeyCode::Up | KeyCode::Char('k') => move_selection(app, -1),

        // Setup guidance is opt-in: the steps appear because this was pressed.
        KeyCode::Enter if app.tab == Tab::Doctor => {
            if app.doctor.current().is_some_and(|c| c.fix.is_some()) {
                app.doctor.mode = DoctorMode::FixDetail;
                app.doctor.command = 0;
            }
        }
        KeyCode::Enter if app.selected_detection().is_some() => app.show_detail = true,
        _ => {}
    }
    Action::Continue
}

fn select_tab(app: &mut App, tab: Tab) {
    if app.tab == tab {
        return;
    }
    app.tab = tab;
    // Each tab lists something different, so a row number carried over from
    // the last one would point at nothing meaningful.
    app.selected = 0;
    app.doctor.selected = 0;
    app.doctor.mode = DoctorMode::List;
    app.show_detail = false;
}

/// Move within the current list, stopping at the ends rather than wrapping —
/// wrapping makes it easy to lose your place in a short list.
fn move_selection(app: &mut App, delta: isize) {
    let count = app.item_count();
    if count == 0 {
        return;
    }

    let last = count - 1;
    // The doctor tab keeps its own cursor, so returning to it lands where the
    // user left off rather than back at the top.
    let cursor = if app.tab == Tab::Doctor {
        &mut app.doctor.selected
    } else {
        &mut app.selected
    };
    *cursor = cursor.saturating_add_signed(delta).min(last);
    app.show_detail = false;
}

/// Keys while a fix guide is open.
///
/// **Windrose never runs an install command.** `c` copies the highlighted one
/// so the user can read it, decide, and run it themselves.
fn update_fix_detail(app: &mut App, key: KeyEvent, sys: &dyn SysCtx) -> Action {
    let command_count = app
        .doctor
        .current()
        .and_then(|c| c.fix.as_ref())
        .map(|f| f.commands.len())
        .unwrap_or(0);

    match key.code {
        KeyCode::Esc => app.doctor.mode = DoctorMode::List,
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Action::Quit;
        }
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char('c') => {
            if let Some(command) = app.doctor.current_command() {
                // Copy only. Running it is the user's decision to make.
                sys.copy_to_clipboard(command);
            }
        }
        KeyCode::Down | KeyCode::Char('j') if command_count > 0 => {
            app.doctor.command = (app.doctor.command + 1).min(command_count - 1);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.doctor.command = app.doctor.command.saturating_sub(1);
        }
        _ => {}
    }
    Action::Continue
}

#[cfg(test)]
pub mod testing {
    //! A fixture app, so update-logic tests need neither a terminal nor a Mac.

    use super::App;
    use crate::hardware::{ChipTier, HardwareProfile};
    use crate::model::{Availability, Category, Detection};
    use crate::report::ScanResult;
    use crate::sys::testing::MockSys;
    use crate::{doctor, scoring};

    impl App {
        pub fn with_fixture() -> Self {
            let hardware = HardwareProfile {
                chip_name: "Apple M4 Pro".to_string(),
                chip_tier: ChipTier::Pro,
                ram_gb: 48,
                gpu_cores: Some(20),
                macos_major: 26,
                is_apple_silicon: true,
                is_laptop: true,
            };

            let detections = vec![
                det(
                    "ollama",
                    "Ollama",
                    Category::LocalRuntime,
                    Availability::Ready,
                ),
                det(
                    "lmstudio",
                    "LM Studio",
                    Category::LocalRuntime,
                    Availability::InstalledNotRunning,
                ),
                det(
                    "mlx",
                    "MLX",
                    Category::OptimisedRuntime,
                    Availability::NotFound,
                ),
                det(
                    "apple-fm",
                    "Apple Foundation Models",
                    Category::ApplePlatform,
                    Availability::Ready,
                ),
                det(
                    "claude",
                    "Claude",
                    Category::CloudProvider,
                    Availability::Ready,
                ),
                det(
                    "groq",
                    "Groq",
                    Category::CloudProvider,
                    Availability::NotFound,
                ),
            ];

            let sys = MockSys::new();
            let health = doctor::health_checks(&detections, &hardware, &sys);
            let perf = doctor::performance_checks(&detections, &hardware, &sys);

            App::new(ScanResult {
                score: scoring::score(&hardware),
                hardware,
                detections,
                health,
                perf,
            })
        }
    }

    fn det(
        id: &'static str,
        name: &'static str,
        category: Category,
        availability: Availability,
    ) -> Detection {
        Detection {
            id,
            name,
            category,
            availability,
            version: Some("1.2.3".to_string()),
            details: vec![("Example row".to_string(), "yes".to_string())],
            friendly: format!("{name} — a plain-English explanation of what this is"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::testing::MockSys;

    fn key(c: char) -> Msg {
        Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    fn key_code(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn tab_and_quit_keys() {
        let mut app = App::with_fixture();

        update(&mut app, key('2'), &MockSys::new());
        assert_eq!(app.tab, Tab::Local);
        update(&mut app, key_code(KeyCode::Tab), &MockSys::new());
        assert_eq!(app.tab, Tab::Cloud);
        assert!(matches!(
            update(&mut app, key('q'), &MockSys::new()),
            Action::Quit
        ));
    }

    #[test]
    fn help_overlay_toggles_on_question_mark() {
        let mut app = App::with_fixture();
        assert!(!app.show_help);

        update(&mut app, key('?'), &MockSys::new());
        assert!(app.show_help);

        // Any key closes it — nobody should have to guess the way out.
        update(&mut app, key('x'), &MockSys::new());
        assert!(!app.show_help);
    }

    /// While help is open it must swallow input, or a key press would both
    /// dismiss the overlay and do something the user could not see coming.
    #[test]
    fn help_swallows_the_key_that_dismisses_it() {
        let mut app = App::with_fixture();
        update(&mut app, key('?'), &MockSys::new());

        update(&mut app, key('3'), &MockSys::new());
        assert!(!app.show_help);
        assert_eq!(app.tab, Tab::Overview, "the tab should not have changed");
    }

    /// Quitting must work from behind the help overlay too.
    #[test]
    fn help_can_be_dismissed_then_quit() {
        let mut app = App::with_fixture();
        update(&mut app, key('?'), &MockSys::new());
        assert!(matches!(
            update(&mut app, key('q'), &MockSys::new()),
            Action::Continue
        ));
        assert!(matches!(
            update(&mut app, key('q'), &MockSys::new()),
            Action::Quit
        ));
    }

    #[test]
    fn number_keys_select_every_tab() {
        let mut app = App::with_fixture();

        for (n, expected) in "12345".chars().zip(Tab::ALL) {
            update(&mut app, key(n), &MockSys::new());
            assert_eq!(app.tab, expected);
        }
    }

    #[test]
    fn tab_cycles_forwards_and_backwards_through_every_tab() {
        let mut app = App::with_fixture();

        for expected in [
            Tab::Local,
            Tab::Cloud,
            Tab::Score,
            Tab::Doctor,
            Tab::Overview,
        ] {
            update(&mut app, key_code(KeyCode::Tab), &MockSys::new());
            assert_eq!(app.tab, expected);
        }
        for expected in [
            Tab::Doctor,
            Tab::Score,
            Tab::Cloud,
            Tab::Local,
            Tab::Overview,
        ] {
            update(&mut app, key_code(KeyCode::BackTab), &MockSys::new());
            assert_eq!(app.tab, expected);
        }
    }

    #[test]
    fn escape_and_ctrl_c_also_quit() {
        let mut app = App::with_fixture();
        assert!(matches!(
            update(&mut app, key_code(KeyCode::Esc), &MockSys::new()),
            Action::Quit
        ));

        let ctrl_c = Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(
            update(&mut app, ctrl_c, &MockSys::new()),
            Action::Quit
        ));
    }

    /// A plain 'c' is not Ctrl-C and must not quit.
    #[test]
    fn plain_c_does_not_quit() {
        let mut app = App::with_fixture();
        assert!(matches!(
            update(&mut app, key('c'), &MockSys::new()),
            Action::Continue
        ));
    }

    #[test]
    fn selection_moves_but_never_runs_off_either_end() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());
        let last = app.item_count() - 1;
        assert!(last > 0, "fixture should have several local options");

        update(&mut app, key_code(KeyCode::Up), &MockSys::new());
        assert_eq!(app.selected, 0, "must not move above the first row");

        for _ in 0..(last + 5) {
            update(&mut app, key_code(KeyCode::Down), &MockSys::new());
        }
        assert_eq!(app.selected, last, "must not move past the last row");
    }

    #[test]
    fn vim_keys_move_the_selection_too() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());

        update(&mut app, key('j'), &MockSys::new());
        assert_eq!(app.selected, 1);
        update(&mut app, key('k'), &MockSys::new());
        assert_eq!(app.selected, 0);
    }

    /// Row 3 of one list has nothing to do with row 3 of another.
    #[test]
    fn changing_tab_resets_the_selection() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());
        update(&mut app, key_code(KeyCode::Down), &MockSys::new());
        assert_eq!(app.selected, 1);

        update(&mut app, key('3'), &MockSys::new());
        assert_eq!(app.selected, 0);
    }

    /// The doctor cursor is reset by a tab change too.
    #[test]
    fn changing_tab_resets_the_doctor_cursor() {
        let mut app = App::with_fixture();
        update(&mut app, key('5'), &MockSys::new());
        update(&mut app, key_code(KeyCode::Down), &MockSys::new());
        assert_eq!(app.doctor.selected, 1);

        update(&mut app, key('2'), &MockSys::new());
        assert_eq!(app.doctor.selected, 0);
        assert_eq!(app.doctor.mode, DoctorMode::List);
    }

    /// Tabs with nothing to select must not move a phantom cursor.
    #[test]
    fn selection_does_nothing_on_tabs_without_lists() {
        let mut app = App::with_fixture();
        update(&mut app, key('1'), &MockSys::new());

        update(&mut app, key_code(KeyCode::Down), &MockSys::new());
        assert_eq!(app.selected, 0);
        assert_eq!(app.item_count(), 0);
    }

    /// Setup guidance is opt-in: the guide opens only on a deliberate press.
    #[test]
    fn a_finding_with_a_fix_opens_its_guide_on_enter() {
        let mut app = App::with_fixture();
        update(&mut app, key('5'), &MockSys::new());
        assert_eq!(
            app.doctor.mode,
            DoctorMode::List,
            "opening a tab is not consent"
        );

        // Land on a finding that failed and therefore has a guide.
        let failing = app
            .doctor
            .results
            .iter()
            .position(|c| c.status == crate::doctor::CheckStatus::Fail)
            .expect("the fixture should contain a failure");
        app.doctor.selected = failing;

        update(&mut app, key_code(KeyCode::Enter), &MockSys::new());
        assert_eq!(app.doctor.mode, DoctorMode::FixDetail);
    }

    /// Nothing to explain means nothing to open.
    #[test]
    fn a_finding_without_a_fix_opens_nothing() {
        let mut app = App::with_fixture();
        update(&mut app, key('5'), &MockSys::new());

        let passing = app
            .doctor
            .results
            .iter()
            .position(|c| c.fix.is_none())
            .expect("the fixture should contain a finding with no guide");
        app.doctor.selected = passing;

        update(&mut app, key_code(KeyCode::Enter), &MockSys::new());
        assert_eq!(app.doctor.mode, DoctorMode::List);
    }

    /// The core safety promise: Windrose copies commands, it never runs them.
    #[test]
    fn c_copies_the_command_and_runs_nothing() {
        let mut app = App::with_fixture();
        let sys = MockSys::new();
        open_first_guide(&mut app, &sys);

        let expected = app
            .doctor
            .current_command()
            .expect("the open guide should offer a command")
            .to_string();

        update(&mut app, key('c'), &sys);

        assert!(
            sys.calls().contains(&format!("pbcopy {expected}")),
            "the command should have been copied; calls were {:?}",
            sys.calls()
        );
        // Nothing was executed. Every recorded call is the clipboard.
        assert!(
            sys.calls().iter().all(|c| c.starts_with("pbcopy ")),
            "Windrose must never run an install command: {:?}",
            sys.calls()
        );
    }

    /// Copying must never be something that happens on its own.
    #[test]
    fn nothing_is_copied_without_pressing_c() {
        let mut app = App::with_fixture();
        let sys = MockSys::new();
        open_first_guide(&mut app, &sys);

        for msg in [key_code(KeyCode::Down), key_code(KeyCode::Up), key('j')] {
            update(&mut app, msg, &sys);
        }
        assert!(
            sys.calls().is_empty(),
            "moving around must not touch the clipboard: {:?}",
            sys.calls()
        );
    }

    #[test]
    fn esc_returns_to_the_list_rather_than_quitting() {
        let mut app = App::with_fixture();
        let sys = MockSys::new();
        open_first_guide(&mut app, &sys);

        assert!(matches!(
            update(&mut app, key_code(KeyCode::Esc), &sys),
            Action::Continue
        ));
        assert_eq!(app.doctor.mode, DoctorMode::List);

        // And from the list, Esc quits as it does everywhere else.
        assert!(matches!(
            update(&mut app, key_code(KeyCode::Esc), &sys),
            Action::Quit
        ));
    }

    /// A guide can list several commands; the cursor picks which one `c` takes.
    #[test]
    fn the_command_cursor_chooses_what_gets_copied() {
        let mut app = App::with_fixture();
        let sys = MockSys::new();
        open_first_guide(&mut app, &sys);

        let commands = app
            .doctor
            .current()
            .and_then(|c| c.fix.as_ref())
            .map(|f| f.commands.clone())
            .unwrap_or_default();
        if commands.len() < 2 {
            return; // nothing to choose between
        }

        update(&mut app, key_code(KeyCode::Down), &sys);
        update(&mut app, key('c'), &sys);
        assert!(sys.calls().contains(&format!("pbcopy {}", commands[1])));
    }

    /// The command cursor must not run off either end of the list.
    #[test]
    fn the_command_cursor_stays_within_the_list() {
        let mut app = App::with_fixture();
        let sys = MockSys::new();
        open_first_guide(&mut app, &sys);

        let count = app
            .doctor
            .current()
            .and_then(|c| c.fix.as_ref())
            .map(|f| f.commands.len())
            .unwrap_or(0);

        for _ in 0..(count + 5) {
            update(&mut app, key_code(KeyCode::Down), &sys);
        }
        assert_eq!(app.doctor.command, count.saturating_sub(1));

        for _ in 0..(count + 5) {
            update(&mut app, key_code(KeyCode::Up), &sys);
        }
        assert_eq!(app.doctor.command, 0);
    }

    /// The doctor tab keeps its own cursor, so leaving and returning does not
    /// silently move the user somewhere else.
    #[test]
    fn the_doctor_cursor_is_separate_from_the_option_lists() {
        let mut app = App::with_fixture();
        update(&mut app, key('5'), &MockSys::new());
        update(&mut app, key_code(KeyCode::Down), &MockSys::new());
        assert_eq!(app.doctor.selected, 1);
        assert_eq!(
            app.selected, 0,
            "the option-list cursor should be untouched"
        );
    }

    /// Open the first finding that has a guide.
    fn open_first_guide(app: &mut App, sys: &MockSys) {
        update(app, key('5'), sys);
        app.doctor.selected = app
            .doctor
            .results
            .iter()
            .position(|c| c.fix.as_ref().is_some_and(|f| !f.commands.is_empty()))
            .expect("the fixture should contain a guide with commands");
        update(app, key_code(KeyCode::Enter), sys);
        assert_eq!(app.doctor.mode, DoctorMode::FixDetail);
    }

    /// Enter on an option list opens that option, not a doctor guide.
    #[test]
    fn enter_outside_the_doctor_tab_opens_no_guide() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());

        update(&mut app, key_code(KeyCode::Enter), &MockSys::new());
        assert_eq!(app.doctor.mode, DoctorMode::List);
        assert!(app.show_detail);
    }

    /// Enter on a listed option opens its detail popup.
    #[test]
    fn enter_opens_the_detail_popup_for_the_selected_option() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());
        assert!(!app.show_detail);

        update(&mut app, key_code(KeyCode::Enter), &MockSys::new());
        assert!(app.show_detail);
        assert_eq!(app.selected_detection().map(|d| d.id), Some("ollama"));
    }

    /// Like the help overlay, the detail popup swallows the key that closes it.
    #[test]
    fn any_key_closes_the_detail_popup_without_acting() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());
        update(&mut app, key_code(KeyCode::Enter), &MockSys::new());

        update(&mut app, key('3'), &MockSys::new());
        assert!(!app.show_detail);
        assert_eq!(app.tab, Tab::Local, "the tab should not have changed");
    }

    /// The popup describes one option, so it must not survive a move.
    #[test]
    fn moving_or_switching_tab_closes_the_detail_popup() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'), &MockSys::new());
        update(&mut app, key_code(KeyCode::Enter), &MockSys::new());
        update(&mut app, key_code(KeyCode::Down), &MockSys::new());
        assert!(!app.show_detail);

        update(&mut app, key_code(KeyCode::Enter), &MockSys::new());
        assert!(app.show_detail);
        update(&mut app, key('3'), &MockSys::new());
        assert!(!app.show_detail);
    }

    /// Tabs without a list have nothing to open.
    #[test]
    fn enter_opens_nothing_on_tabs_without_options() {
        let mut app = App::with_fixture();
        for tab_key in ['1', '4'] {
            update(&mut app, key(tab_key), &MockSys::new());
            update(&mut app, key_code(KeyCode::Enter), &MockSys::new());
            assert!(!app.show_detail, "opened a popup on {}", app.tab.title());
        }
    }

    #[test]
    fn selected_detection_follows_the_cursor() {
        let mut app = App::with_fixture();
        update(&mut app, key('3'), &MockSys::new());

        assert_eq!(app.selected_detection().map(|d| d.id), Some("claude"));
        update(&mut app, key_code(KeyCode::Down), &MockSys::new());
        assert_eq!(app.selected_detection().map(|d| d.id), Some("groq"));
    }

    #[test]
    fn a_tick_changes_nothing() {
        let mut app = App::with_fixture();
        let (tab, selected, help) = (app.tab, app.selected, app.show_help);

        assert!(matches!(
            update(&mut app, Msg::Tick, &MockSys::new()),
            Action::Continue
        ));
        assert_eq!(
            (app.tab, app.selected, app.show_help),
            (tab, selected, help)
        );
    }
}
