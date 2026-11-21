use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ModalKind, ScrollTarget};

use super::model::{Binding, Row, hints};
use super::scroll::line_scroll;
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut scroll = line_scroll(ScrollTarget::SqliteDiagnostics);
    scroll.surfaces = &[
        HintSurface::Border,
        HintSurface::Current,
        HintSurface::Advanced,
    ];
    vec![
        scroll,
        Row {
            bindings: vec![Binding::action(
                Action::RunSqliteDiagnosticsQuickCheck,
                &[KeyCombo::plain(Key::Char('r'))],
            )],
            short: "Run check",
            description: "Run quick check",
            references: &[],
            dynamic_label: None,
            visible: Some(|context, surface| {
                surface != HintSurface::Border || context.can_run_diagnostics()
            }),
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        },
        Row {
            bindings: vec![Binding::action(
                Action::ToggleHelp,
                &[KeyCombo::plain(Key::Char('?'))],
            )],
            short: "Help",
            description: "Toggle help",
            references: &[],
            dynamic_label: None,
            visible: Some(|context, surface| {
                surface != HintSurface::Footer || context.supports_diagnostics()
            }),
            surfaces: &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        },
        Row::new(
            vec![Binding::action(
                Action::CloseModal(ModalKind::SqliteDiagnostics),
                &[KeyCombo::plain(Key::Esc)],
            )],
            "Close",
            "Close",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ),
    ]
});

pub fn sqlite_diagnostics_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConnectionId, DatabaseType, DiagnosticField, SqliteDiagnosticsSnapshot};
    use crate::model::app_state::AppState;
    use crate::model::shared::input_mode::InputMode;
    use crate::test_support::endpoint;

    #[test]
    fn quick_check_hint_follows_loading_without_changing_key_resolution() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection(
            &ConnectionId::new(),
            "database",
            &endpoint::for_engine(DatabaseType::SQLite, "sqlite://test.db"),
        );
        state.modal.set_mode(InputMode::SqliteDiagnostics);
        let run_id = state.sqlite_diagnostics.begin_core_fetch();
        let context = KeyContext::from_state(&state);
        assert!(
            !sqlite_diagnostics_hints(HintSurface::Border, &context)
                .iter()
                .any(|hint| hint.short == "Run check")
        );
        assert!(matches!(
            super::super::key_action(KeyCombo::plain(Key::Char('r')), &context),
            Action::RunSqliteDiagnosticsQuickCheck
        ));

        state.sqlite_diagnostics.set_core_loaded(
            run_id,
            SqliteDiagnosticsSnapshot::core_fetch_failed(DiagnosticField::Unavailable),
        );
        let context = KeyContext::from_state(&state);
        assert!(
            sqlite_diagnostics_hints(HintSurface::Border, &context)
                .iter()
                .any(|hint| hint.short == "Run check")
        );
    }
}
