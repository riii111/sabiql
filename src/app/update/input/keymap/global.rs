use std::sync::LazyLock;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::model::app_state::AppState;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, ModalKind};

use super::context::BrowsePane;
use super::model::{Binding, Row, indexed_hints, retained_hint, row_action};
use super::{HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    vec![
        footer(
            row(Action::Quit, 'q', "Quit", "Quit application"),
            HintSurface::FooterQuit,
            None,
        ),
        footer(
            row(Action::ToggleHelp, '?', "Help", "Toggle help"),
            HintSurface::FooterHelp,
            None,
        ),
        footer(
            preset_row(
                Action::OpenModal(ModalKind::TablePicker),
                'p',
                'T',
                "Tables",
                "Open Table Picker",
            ),
            HintSurface::FooterAction,
            Some(regular_footer),
        ),
        footer(
            row(
                Action::OpenModal(ModalKind::Settings),
                ',',
                "Settings",
                "Open Settings",
            ),
            HintSurface::FooterHelp,
            Some(non_cell_footer),
        ),
        Row {
            surfaces: &[],
            ..row(Action::EnterCommandLine, ':', "Cmd", "Enter command line")
        },
        footer(
            Row {
                bindings: vec![
                    Binding::action(
                        Action::OpenModal(ModalKind::CommandPalette),
                        &[KeyCombo::plain(Key::F(1))],
                    )
                    .ide(&[KeyCombo::plain(Key::Char('P'))]),
                ],
                short: "Palette",
                description: "Open Command Palette",
                references: &[HintSurface::Common],
                dynamic_label: None,
                visible: None,
                surfaces: &[],
            },
            HintSurface::FooterHelp,
            Some(regular_footer),
        ),
        Row {
            references: &[HintSurface::Common],
            dynamic_label: Some(|context| {
                if context.browse_focused() {
                    ("Exit Focus", "Exit Focus mode")
                } else {
                    ("Focus", "Toggle Focus mode")
                }
            }),
            visible: Some(|context, surface| match surface {
                HintSurface::FooterAction => {
                    !context.browse_focused() && context.browse_pane() != BrowsePane::ResultCell
                }
                HintSurface::FooterClose => {
                    context.browse_focused() && context.browse_pane() != BrowsePane::ResultCell
                }
                _ => true,
            }),
            surfaces: &[
                HintSurface::Palette,
                HintSurface::FooterAction,
                HintSurface::FooterClose,
            ],
            ..row(Action::ToggleFocus, 'f', "Focus", "Toggle Focus mode")
        },
        footer(
            reference(
                row(Action::ReloadMetadata, 'r', "Reload", "Reload metadata"),
                &[HintSurface::Current, HintSurface::DataActions],
            ),
            HintSurface::FooterAction,
            Some(|context, surface| {
                !matches!(
                    surface,
                    HintSurface::FooterAction | HintSurface::Palette | HintSurface::Current
                ) || (regular_footer(context, surface) && context.browse_connected())
            }),
        ),
        footer(
            reference(
                row(
                    Action::OpenModal(ModalKind::SqlModal),
                    's',
                    "SQL",
                    "Open SQL Editor",
                ),
                &[HintSurface::Current, HintSurface::OpenSwitch],
            ),
            HintSurface::FooterAction,
            Some(regular_footer),
        ),
        footer(
            reference(
                row(
                    Action::OpenModal(ModalKind::ErTablePicker),
                    'e',
                    "ER Diagram",
                    "Open ER Diagram",
                ),
                &[HintSurface::Current, HintSurface::OpenSwitch],
            ),
            HintSurface::FooterAction,
            Some(regular_footer),
        ),
        footer(
            reference(
                row(
                    Action::OpenModal(ModalKind::ConnectionSelector),
                    'c',
                    "Connections",
                    "Open Connection Selector",
                ),
                &[HintSurface::Current, HintSurface::OpenSwitch],
            ),
            HintSurface::FooterAction,
            None,
        ),
        footer(
            Row {
                bindings: vec![
                    Binding::action(
                        Action::OpenModal(ModalKind::SqliteDiagnostics),
                        &[KeyCombo::ctrl_shift(Key::Char('d'))],
                    )
                    .ide(&[KeyCombo::plain(Key::Char('D'))]),
                ],
                short: "Diagnostics",
                description: "Open SQLite Diagnostics",
                references: &[HintSurface::Current, HintSurface::OpenSwitch],
                dynamic_label: None,
                visible: None,
                surfaces: &[HintSurface::Palette],
            },
            HintSurface::FooterAction,
            Some(regular_footer),
        ),
        footer(
            Row {
                bindings: vec![
                    Binding::action(Action::RequestCsvExport, &[KeyCombo::ctrl(Key::Char('e'))])
                        .ide(&[KeyCombo::plain(Key::Char('E'))])
                        .when(KeyContext::browse_csv_available),
                ],
                short: "Export",
                description: "Export result to CSV",
                references: &[HintSurface::Current, HintSurface::DataActions],
                dynamic_label: None,
                visible: None,
                surfaces: &[HintSurface::Palette],
            },
            HintSurface::FooterAction,
            Some(non_cell_footer),
        ),
        footer(
            Row {
                references: &[HintSurface::Common],
                dynamic_label: Some(|context| {
                    if context.browse_read_only() {
                        ("Read-Write", "Disable Read-Only mode")
                    } else {
                        ("Read-Only", "Enable Read-Only mode")
                    }
                }),
                ..preset_row(
                    Action::ToggleReadOnly,
                    'r',
                    'R',
                    "Read-Only",
                    "Enable Read-Only mode",
                )
            },
            HintSurface::FooterAction,
            Some(regular_footer),
        ),
        footer(
            Row {
                bindings: vec![
                    Binding::action(
                        Action::OpenModal(ModalKind::QueryHistoryPicker),
                        &[KeyCombo::ctrl(Key::Char('o'))],
                    )
                    .ide(&[KeyCombo::plain(Key::Char('O'))])
                    .when(KeyContext::can_open_history),
                ],
                short: "History",
                description: "Open Query History",
                references: &[HintSurface::Current, HintSurface::OpenSwitch],
                dynamic_label: None,
                visible: None,
                surfaces: &[HintSurface::Palette],
            },
            HintSurface::FooterAction,
            Some(regular_footer),
        ),
    ]
});

fn regular_footer(context: &KeyContext, surface: HintSurface) -> bool {
    !matches!(surface, HintSurface::FooterAction | HintSurface::FooterHelp)
        || (!context.browse_focused() && context.browse_pane() != BrowsePane::ResultCell)
}

fn non_cell_footer(context: &KeyContext, surface: HintSurface) -> bool {
    !matches!(surface, HintSurface::FooterAction | HintSurface::FooterHelp)
        || context.browse_pane() != BrowsePane::ResultCell
}

fn reference(mut row: Row, references: &'static [HintSurface]) -> Row {
    row.references = references;
    row
}

fn footer(
    mut row: Row,
    group: HintSurface,
    visible: Option<fn(&KeyContext, HintSurface) -> bool>,
) -> Row {
    row.surfaces = match (row.surfaces.contains(&HintSurface::Palette), group) {
        (true, HintSurface::FooterAction) => &[HintSurface::Palette, HintSurface::FooterAction],
        (true, HintSurface::FooterHelp) => &[HintSurface::Palette, HintSurface::FooterHelp],
        (true, HintSurface::FooterQuit) => &[HintSurface::Palette, HintSurface::FooterQuit],
        (false, HintSurface::FooterHelp) => &[HintSurface::FooterHelp],
        _ => unreachable!(),
    };
    row.visible = visible;
    row
}

fn row(action: Action, key: char, short: &'static str, description: &'static str) -> Row {
    Row {
        bindings: vec![Binding::action(action, &[KeyCombo::plain(Key::Char(key))])],
        short,
        description,
        references: &[HintSurface::Common],
        dynamic_label: None,
        visible: None,
        surfaces: &[HintSurface::Palette],
    }
}

fn preset_row(
    action: Action,
    default: char,
    ide: char,
    short: &'static str,
    description: &'static str,
) -> Row {
    Row {
        bindings: vec![
            Binding::action(action, &[KeyCombo::ctrl(Key::Char(default))])
                .ide(&[KeyCombo::plain(Key::Char(ide))]),
        ],
        short,
        description,
        references: &[HintSurface::Current, HintSurface::OpenSwitch],
        dynamic_label: None,
        visible: None,
        surfaces: &[HintSurface::Palette],
    }
}

#[derive(Clone)]
pub struct PaletteEntry {
    pub id: usize,
    pub key: String,
    pub description: &'static str,
    pub action: Action,
}

pub fn palette_available(context: &KeyContext) -> Vec<PaletteEntry> {
    indexed_hints(&ROWS, context, HintSurface::Palette)
        .into_iter()
        .filter_map(|(id, hint)| {
            Some(PaletteEntry {
                id,
                key: hint.key,
                description: hint.description,
                action: row_action(&ROWS[id])?,
            })
        })
        .collect()
}

pub fn palette_entry(id: usize, context: &KeyContext) -> Option<PaletteEntry> {
    let row = ROWS.get(id)?;
    if !row.surfaces.contains(&HintSurface::Palette) {
        return None;
    }
    let hint = retained_hint(row, context, HintSurface::Palette);
    Some(PaletteEntry {
        id,
        key: hint.key,
        description: hint.description,
        action: row_action(row)?,
    })
}

/// Entries among `commands` whose description fuzzy-matches `filter`, best match first.
/// Ties and an empty filter keep the palette order.
fn palette_matches(commands: &[usize], filter: &str, context: &KeyContext) -> Vec<PaletteEntry> {
    let entries = commands.iter().filter_map(|&id| palette_entry(id, context));
    if filter.is_empty() {
        return entries.collect();
    }

    let pattern = Pattern::parse(filter, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, PaletteEntry)> = entries
        .filter_map(|entry| {
            let haystack = Utf32Str::new(entry.description, &mut buf);
            pattern
                .score(haystack, &mut matcher)
                .map(|score| (score, entry))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, entry)| entry).collect()
}

pub fn listed_palette_entries(state: &AppState) -> Vec<PaletteEntry> {
    palette_matches(
        state.ui.palette_commands(),
        state.ui.table_picker().filter_input().content(),
        &KeyContext::for_browse_state(state),
    )
}

pub fn palette_action(id: usize, context: &KeyContext) -> Option<Action> {
    palette_available(context)
        .into_iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConnectionId, DatabaseType, QueryResult, QuerySource};
    use crate::test_support::endpoint;
    use std::sync::Arc;
    use std::time::Instant;

    fn palette_ids(context: &KeyContext) -> Vec<usize> {
        palette_available(context)
            .into_iter()
            .map(|entry| entry.id)
            .collect()
    }

    #[test]
    fn empty_filter_keeps_every_command_in_order() {
        let state = AppState::new("test".to_string());
        let context = KeyContext::for_browse_state(&state);
        let ids = palette_ids(&context);

        let matches = palette_matches(&ids, "", &context);

        assert_eq!(
            matches.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            ids
        );
    }

    #[test]
    fn fuzzy_filter_ranks_matching_description_first() {
        let state = AppState::new("test".to_string());
        let context = KeyContext::for_browse_state(&state);

        let matches = palette_matches(&palette_ids(&context), "opn sett", &context);

        assert_eq!(
            matches.first().map(|entry| entry.description),
            Some("Open Settings")
        );
    }

    #[test]
    fn filter_without_matches_yields_no_commands() {
        let state = AppState::new("test".to_string());
        let context = KeyContext::for_browse_state(&state);

        let matches = palette_matches(&palette_ids(&context), "zzzzqx", &context);

        assert!(matches.is_empty());
    }

    #[test]
    fn palette_checks_connection_and_history_availability_again_on_selection() {
        let mut state = AppState::new("test".to_string());
        state
            .query
            .set_current_result(Arc::new(QueryResult::success(
                "SELECT 1".to_string(),
                vec![],
                vec![],
                1,
                QuerySource::Adhoc,
            )));
        let unavailable = palette_available(&KeyContext::for_browse_state(&state));
        assert!(!unavailable.iter().any(|entry| matches!(
            entry.action,
            Action::ReloadMetadata | Action::OpenModal(ModalKind::QueryHistoryPicker)
        )));
        assert!(
            !unavailable
                .iter()
                .any(|entry| matches!(entry.action, Action::RequestCsvExport))
        );

        state.session.activate_connection(
            &ConnectionId::new(),
            "database",
            &endpoint::for_engine(DatabaseType::PostgreSQL, "postgres://localhost/test"),
        );
        let available = palette_available(&KeyContext::for_browse_state(&state));
        let reload = available
            .iter()
            .find(|entry| matches!(entry.action, Action::ReloadMetadata))
            .unwrap()
            .id;
        let history = available
            .iter()
            .find(|entry| {
                matches!(
                    entry.action,
                    Action::OpenModal(ModalKind::QueryHistoryPicker)
                )
            })
            .unwrap()
            .id;
        let _ = state.query.begin_running(Instant::now());
        assert!(palette_action(history, &KeyContext::for_browse_state(&state)).is_none());
        state.session.clear_connection();
        assert!(palette_action(reload, &KeyContext::for_browse_state(&state)).is_none());
    }

    #[test]
    fn palette_uses_the_current_read_only_label_once() {
        let mut state = AppState::new("test".to_string());
        for (read_only, description) in [
            (false, "Enable Read-Only mode"),
            (true, "Disable Read-Only mode"),
        ] {
            if read_only {
                state.session.enable_read_only();
            }
            let commands = palette_available(&KeyContext::for_browse_state(&state));
            let toggles: Vec<_> = commands
                .iter()
                .filter(|entry| matches!(entry.action, Action::ToggleReadOnly))
                .collect();
            assert_eq!(toggles.len(), 1);
            assert_eq!(toggles[0].description, description);
            assert!(
                !commands
                    .iter()
                    .any(|entry| matches!(entry.action, Action::RequestCsvExport))
            );
        }
    }
}
