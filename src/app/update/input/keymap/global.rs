use std::sync::LazyLock;

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
                dynamic_label: None,
                visible: None,
                surfaces: &[],
            },
            HintSurface::FooterHelp,
            Some(regular_footer),
        ),
        Row {
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
            row(Action::ReloadMetadata, 'r', "Reload", "Reload metadata"),
            HintSurface::FooterAction,
            Some(|context, surface| {
                surface == HintSurface::Palette
                    || (regular_footer(context, surface) && context.browse_connected())
            }),
        ),
        footer(
            row(
                Action::OpenModal(ModalKind::SqlModal),
                's',
                "SQL",
                "Open SQL Editor",
            ),
            HintSurface::FooterAction,
            Some(regular_footer),
        ),
        footer(
            row(
                Action::OpenModal(ModalKind::ErTablePicker),
                'e',
                "ER Diagram",
                "Open ER Diagram",
            ),
            HintSurface::FooterAction,
            Some(regular_footer),
        ),
        footer(
            row(
                Action::OpenModal(ModalKind::ConnectionSelector),
                'c',
                "Connections",
                "Open Connection Selector",
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
                ..preset_row(
                    Action::RequestCsvExport,
                    'e',
                    'E',
                    "Export",
                    "Export result to CSV",
                )
            },
            HintSurface::FooterAction,
            Some(non_cell_footer),
        ),
        footer(
            Row {
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
            preset_row(
                Action::OpenModal(ModalKind::QueryHistoryPicker),
                'o',
                'O',
                "History",
                "Open Query History",
            ),
            HintSurface::FooterAction,
            Some(regular_footer),
        ),
    ]
});

fn regular_footer(context: &KeyContext, surface: HintSurface) -> bool {
    surface == HintSurface::Palette
        || (!context.browse_focused() && context.browse_pane() != BrowsePane::ResultCell)
}

fn non_cell_footer(context: &KeyContext, surface: HintSurface) -> bool {
    surface == HintSurface::Palette || context.browse_pane() != BrowsePane::ResultCell
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

pub fn palette_action(id: usize, context: &KeyContext) -> Option<Action> {
    palette_available(context)
        .into_iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::app_state::AppState;

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
