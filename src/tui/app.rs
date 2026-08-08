//! The state and the state machine.
//!
//! Elm architecture: all state lives in [`App`], every input becomes a [`Msg`],
//! and [`update`] is the only thing that changes state. It touches no terminal
//! and no operating system, so the whole interaction model is testable without
//! drawing anything.

use crate::model::Category;
use crate::report::ScanResult;
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

/// Doctor-tab state. The wizard in Task 14 extends this.
#[derive(Debug, Clone, Default)]
pub struct DoctorState {
    /// Whether the setup steps for the selected finding are on screen.
    ///
    /// Setup guidance is opt-in, so this starts false and only ever becomes
    /// true because the user pressed a key.
    pub showing_fix: bool,
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
        Self {
            scan,
            tab: Tab::Overview,
            selected: 0,
            show_help: false,
            show_detail: false,
            doctor: DoctorState::default(),
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
            Tab::Doctor => self.scan.health.len() + self.scan.perf.len(),
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
pub fn update(app: &mut App, msg: Msg) -> Action {
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
            app.doctor.showing_fix = !app.doctor.showing_fix;
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
    app.doctor.showing_fix = false;
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
    app.selected = app.selected.saturating_add_signed(delta).min(last);
    app.doctor.showing_fix = false;
    app.show_detail = false;
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

    fn key(c: char) -> Msg {
        Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    fn key_code(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn tab_and_quit_keys() {
        let mut app = App::with_fixture();

        update(&mut app, key('2'));
        assert_eq!(app.tab, Tab::Local);
        update(&mut app, key_code(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Cloud);
        assert!(matches!(update(&mut app, key('q')), Action::Quit));
    }

    #[test]
    fn help_overlay_toggles_on_question_mark() {
        let mut app = App::with_fixture();
        assert!(!app.show_help);

        update(&mut app, key('?'));
        assert!(app.show_help);

        // Any key closes it — nobody should have to guess the way out.
        update(&mut app, key('x'));
        assert!(!app.show_help);
    }

    /// While help is open it must swallow input, or a key press would both
    /// dismiss the overlay and do something the user could not see coming.
    #[test]
    fn help_swallows_the_key_that_dismisses_it() {
        let mut app = App::with_fixture();
        update(&mut app, key('?'));

        update(&mut app, key('3'));
        assert!(!app.show_help);
        assert_eq!(app.tab, Tab::Overview, "the tab should not have changed");
    }

    /// Quitting must work from behind the help overlay too.
    #[test]
    fn help_can_be_dismissed_then_quit() {
        let mut app = App::with_fixture();
        update(&mut app, key('?'));
        assert!(matches!(update(&mut app, key('q')), Action::Continue));
        assert!(matches!(update(&mut app, key('q')), Action::Quit));
    }

    #[test]
    fn number_keys_select_every_tab() {
        let mut app = App::with_fixture();

        for (n, expected) in "12345".chars().zip(Tab::ALL) {
            update(&mut app, key(n));
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
            update(&mut app, key_code(KeyCode::Tab));
            assert_eq!(app.tab, expected);
        }
        for expected in [
            Tab::Doctor,
            Tab::Score,
            Tab::Cloud,
            Tab::Local,
            Tab::Overview,
        ] {
            update(&mut app, key_code(KeyCode::BackTab));
            assert_eq!(app.tab, expected);
        }
    }

    #[test]
    fn escape_and_ctrl_c_also_quit() {
        let mut app = App::with_fixture();
        assert!(matches!(
            update(&mut app, key_code(KeyCode::Esc)),
            Action::Quit
        ));

        let ctrl_c = Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(update(&mut app, ctrl_c), Action::Quit));
    }

    /// A plain 'c' is not Ctrl-C and must not quit.
    #[test]
    fn plain_c_does_not_quit() {
        let mut app = App::with_fixture();
        assert!(matches!(update(&mut app, key('c')), Action::Continue));
    }

    #[test]
    fn selection_moves_but_never_runs_off_either_end() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'));
        let last = app.item_count() - 1;
        assert!(last > 0, "fixture should have several local options");

        update(&mut app, key_code(KeyCode::Up));
        assert_eq!(app.selected, 0, "must not move above the first row");

        for _ in 0..(last + 5) {
            update(&mut app, key_code(KeyCode::Down));
        }
        assert_eq!(app.selected, last, "must not move past the last row");
    }

    #[test]
    fn vim_keys_move_the_selection_too() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'));

        update(&mut app, key('j'));
        assert_eq!(app.selected, 1);
        update(&mut app, key('k'));
        assert_eq!(app.selected, 0);
    }

    /// Row 3 of one list has nothing to do with row 3 of another.
    #[test]
    fn changing_tab_resets_the_selection() {
        let mut app = App::with_fixture();
        update(&mut app, key('5'));
        update(&mut app, key_code(KeyCode::Down));
        assert_eq!(app.selected, 1);

        update(&mut app, key('3'));
        assert_eq!(app.selected, 0);
    }

    /// Tabs with nothing to select must not move a phantom cursor.
    #[test]
    fn selection_does_nothing_on_tabs_without_lists() {
        let mut app = App::with_fixture();
        update(&mut app, key('1'));

        update(&mut app, key_code(KeyCode::Down));
        assert_eq!(app.selected, 0);
        assert_eq!(app.item_count(), 0);
    }

    /// Setup guidance is opt-in: it appears only after a deliberate key press.
    #[test]
    fn fix_steps_are_hidden_until_the_user_asks() {
        let mut app = App::with_fixture();
        assert!(!app.doctor.showing_fix);

        update(&mut app, key('5'));
        assert!(!app.doctor.showing_fix, "opening the tab is not consent");

        update(&mut app, key_code(KeyCode::Enter));
        assert!(app.doctor.showing_fix);
        update(&mut app, key_code(KeyCode::Enter));
        assert!(!app.doctor.showing_fix);
    }

    /// Steps shown for one finding must not stay open over another.
    #[test]
    fn moving_to_another_finding_closes_the_open_steps() {
        let mut app = App::with_fixture();
        update(&mut app, key('5'));
        update(&mut app, key_code(KeyCode::Enter));
        assert!(app.doctor.showing_fix);

        update(&mut app, key_code(KeyCode::Down));
        assert!(!app.doctor.showing_fix);
    }

    /// Enter belongs to the doctor tab; elsewhere it must do nothing.
    #[test]
    fn enter_does_nothing_outside_the_doctor_tab() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'));

        update(&mut app, key_code(KeyCode::Enter));
        assert!(!app.doctor.showing_fix);
    }

    /// Enter on a listed option opens its detail popup.
    #[test]
    fn enter_opens_the_detail_popup_for_the_selected_option() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'));
        assert!(!app.show_detail);

        update(&mut app, key_code(KeyCode::Enter));
        assert!(app.show_detail);
        assert_eq!(app.selected_detection().map(|d| d.id), Some("ollama"));
    }

    /// Like the help overlay, the detail popup swallows the key that closes it.
    #[test]
    fn any_key_closes_the_detail_popup_without_acting() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'));
        update(&mut app, key_code(KeyCode::Enter));

        update(&mut app, key('3'));
        assert!(!app.show_detail);
        assert_eq!(app.tab, Tab::Local, "the tab should not have changed");
    }

    /// The popup describes one option, so it must not survive a move.
    #[test]
    fn moving_or_switching_tab_closes_the_detail_popup() {
        let mut app = App::with_fixture();
        update(&mut app, key('2'));
        update(&mut app, key_code(KeyCode::Enter));
        update(&mut app, key_code(KeyCode::Down));
        assert!(!app.show_detail);

        update(&mut app, key_code(KeyCode::Enter));
        assert!(app.show_detail);
        update(&mut app, key('3'));
        assert!(!app.show_detail);
    }

    /// Tabs without a list have nothing to open.
    #[test]
    fn enter_opens_nothing_on_tabs_without_options() {
        let mut app = App::with_fixture();
        for tab_key in ['1', '4'] {
            update(&mut app, key(tab_key));
            update(&mut app, key_code(KeyCode::Enter));
            assert!(!app.show_detail, "opened a popup on {}", app.tab.title());
        }
    }

    #[test]
    fn selected_detection_follows_the_cursor() {
        let mut app = App::with_fixture();
        update(&mut app, key('3'));

        assert_eq!(app.selected_detection().map(|d| d.id), Some("claude"));
        update(&mut app, key_code(KeyCode::Down));
        assert_eq!(app.selected_detection().map(|d| d.id), Some("groq"));
    }

    #[test]
    fn a_tick_changes_nothing() {
        let mut app = App::with_fixture();
        let (tab, selected, help) = (app.tab, app.selected, app.show_help);

        assert!(matches!(update(&mut app, Msg::Tick), Action::Continue));
        assert_eq!(
            (app.tab, app.selected, app.show_help),
            (tab, selected, help)
        );
    }
}
