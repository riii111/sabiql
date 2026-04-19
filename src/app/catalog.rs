use std::borrow::Cow;

use crate::model::app_state::AppState;
use crate::model::er_state::ErStatus;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::model::shared::ui_state::ResultNavMode;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::services::AppServices;
use crate::update::input::keybindings::{
    CELL_EDIT_KEYS, COMMAND_LINE_KEYS, COMMAND_PALETTE_ROWS, CONFIRM_DIALOG_KEYS,
    CONNECTION_ERROR_ROWS, CONNECTION_SELECTOR_ROWS, CONNECTION_SETUP_KEYS, ER_PICKER_ROWS,
    FOOTER_NAV_KEYS, GLOBAL_KEYS, HELP_ROWS, HISTORY_KEYS, INSPECTOR_DDL_KEYS, JSONB_DETAIL_ROWS,
    JSONB_EDIT_ROWS, JSONB_SEARCH_KEYS, KeyBinding, ModeRow, NAVIGATION_KEYS, OVERLAY_KEYS,
    QUERY_HISTORY_PICKER_ROWS, RESULT_ACTIVE_KEYS, SQL_MODAL_COMPARE_KEYS,
    SQL_MODAL_CONFIRMING_KEYS, SQL_MODAL_KEYS, SQL_MODAL_NORMAL_KEYS, SQL_MODAL_PLAN_KEYS,
    TABLE_PICKER_ROWS, idx,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hint {
    pub key: &'static str,
    pub description: &'static str,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HelpEntry {
    pub key: &'static str,
    pub description: Cow<'static, str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpSection {
    pub title: &'static str,
    pub entries: Vec<HelpEntry>,
}

fn hint_from_binding(binding: &KeyBinding) -> Hint {
    let (key, description) = binding.as_hint();
    Hint { key, description }
}

fn hint_from_row(row: &ModeRow) -> Hint {
    let (key, description) = row.as_hint();
    Hint { key, description }
}

fn help_entry_from_binding(binding: &KeyBinding) -> HelpEntry {
    HelpEntry {
        key: binding.key,
        description: Cow::Borrowed(binding.description),
    }
}

fn help_entry_from_row(row: &ModeRow) -> HelpEntry {
    HelpEntry {
        key: row.key,
        description: Cow::Borrowed(row.description),
    }
}

fn dedup_adjacent_bindings(bindings: &[KeyBinding]) -> Vec<HelpEntry> {
    let mut entries = Vec::new();
    let mut i = 0;
    while i < bindings.len() {
        let run_end = bindings[i..]
            .iter()
            .position(|binding| binding.key != bindings[i].key)
            .map_or(bindings.len(), |offset| i + offset);

        if run_end - i >= 2 {
            entries.push(HelpEntry {
                key: bindings[i].key,
                description: Cow::Owned(format!("Toggle {}", bindings[i].desc_short)),
            });
        } else {
            entries.push(help_entry_from_binding(&bindings[i]));
        }
        i = run_end;
    }
    entries
}

pub fn help_sections() -> Vec<HelpSection> {
    vec![
        HelpSection {
            title: "Global Keys",
            entries: dedup_adjacent_bindings(GLOBAL_KEYS),
        },
        HelpSection {
            title: "Navigation",
            entries: NAVIGATION_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "Result History",
            entries: dedup_adjacent_bindings(HISTORY_KEYS),
        },
        HelpSection {
            title: "Result Pane",
            entries: RESULT_ACTIVE_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "Inspector Pane (DDL tab)",
            entries: INSPECTOR_DDL_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "Cell Edit",
            entries: CELL_EDIT_KEYS.iter().map(help_entry_from_binding).collect(),
        },
        HelpSection {
            title: "SQL Editor (Normal)",
            entries: SQL_MODAL_NORMAL_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "SQL Editor (Insert)",
            entries: SQL_MODAL_KEYS.iter().map(help_entry_from_binding).collect(),
        },
        HelpSection {
            title: "SQL Editor (Plan)",
            entries: SQL_MODAL_PLAN_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "SQL Editor (Compare)",
            entries: SQL_MODAL_COMPARE_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "SQL Editor (Confirm)",
            entries: SQL_MODAL_CONFIRMING_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "Overlays",
            entries: OVERLAY_KEYS.iter().map(help_entry_from_binding).collect(),
        },
        HelpSection {
            title: "Command Line",
            entries: COMMAND_LINE_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "Connection Setup",
            entries: CONNECTION_SETUP_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "Connection Error",
            entries: CONNECTION_ERROR_ROWS
                .iter()
                .map(help_entry_from_row)
                .collect(),
        },
        HelpSection {
            title: "Connection Selector",
            entries: CONNECTION_SELECTOR_ROWS
                .iter()
                .map(help_entry_from_row)
                .collect(),
        },
        HelpSection {
            title: "ER Diagram Picker",
            entries: ER_PICKER_ROWS.iter().map(help_entry_from_row).collect(),
        },
        HelpSection {
            title: "Query History Picker",
            entries: QUERY_HISTORY_PICKER_ROWS
                .iter()
                .map(help_entry_from_row)
                .collect(),
        },
        HelpSection {
            title: "Table Picker",
            entries: TABLE_PICKER_ROWS.iter().map(help_entry_from_row).collect(),
        },
        HelpSection {
            title: "Command Palette",
            entries: COMMAND_PALETTE_ROWS
                .iter()
                .map(help_entry_from_row)
                .collect(),
        },
        HelpSection {
            title: "Help Overlay",
            entries: HELP_ROWS.iter().map(help_entry_from_row).collect(),
        },
        HelpSection {
            title: "Confirm Dialog",
            entries: CONFIRM_DIALOG_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
        HelpSection {
            title: "JSONB Detail",
            entries: JSONB_DETAIL_ROWS.iter().map(help_entry_from_row).collect(),
        },
        HelpSection {
            title: "JSONB Edit",
            entries: JSONB_EDIT_ROWS.iter().map(help_entry_from_row).collect(),
        },
        HelpSection {
            title: "JSONB Search",
            entries: JSONB_SEARCH_KEYS
                .iter()
                .map(help_entry_from_binding)
                .collect(),
        },
    ]
}

pub fn footer_status_text(state: &AppState, time_ms: u128) -> Option<String> {
    if state.er_preparation.status != ErStatus::Waiting {
        return None;
    }

    let spinner = spinner_char(time_ms);
    let total = state.er_preparation.total_tables;
    let failed_count = state.er_preparation.failed_tables.len();
    let remaining =
        state.er_preparation.pending_tables.len() + state.er_preparation.fetching_tables.len();
    let cached = total.saturating_sub(remaining + failed_count);
    Some(format!("{spinner} Preparing ER... ({cached}/{total})"))
}

fn spinner_char(time_ms: u128) -> &'static str {
    const SPINNER_FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
    let idx = (time_ms / 300) % SPINNER_FRAMES.len() as u128;
    SPINNER_FRAMES[idx as usize]
}

pub fn footer_hints(state: &AppState, services: &AppServices) -> Vec<Hint> {
    match state.input_mode() {
        InputMode::Normal => normal_mode_footer_hints(state, services),
        InputMode::CommandLine => vec![
            hint_from_binding(&OVERLAY_KEYS[idx::overlay::ENTER_EXECUTE]),
            hint_from_binding(&OVERLAY_KEYS[idx::overlay::ESC_CANCEL]),
        ],
        InputMode::CellEdit => vec![
            hint_from_binding(&CELL_EDIT_KEYS[idx::cell_edit::WRITE]),
            hint_from_binding(&CELL_EDIT_KEYS[idx::cell_edit::TYPE]),
            hint_from_binding(&CELL_EDIT_KEYS[idx::cell_edit::MOVE]),
            hint_from_binding(&GLOBAL_KEYS[idx::global::HELP]),
            hint_from_binding(&CELL_EDIT_KEYS[idx::cell_edit::ESC_CANCEL]),
            hint_from_binding(&GLOBAL_KEYS[idx::global::QUIT]),
        ],
        InputMode::TablePicker => vec![
            hint_from_row(&TABLE_PICKER_ROWS[idx::table_picker::ENTER_SELECT]),
            hint_from_row(&TABLE_PICKER_ROWS[idx::table_picker::TYPE_FILTER]),
            hint_from_row(&TABLE_PICKER_ROWS[idx::table_picker::ESC_CLOSE]),
        ],
        InputMode::CommandPalette => vec![
            hint_from_row(&COMMAND_PALETTE_ROWS[idx::cmd_palette::ENTER_EXECUTE]),
            hint_from_row(&COMMAND_PALETTE_ROWS[idx::cmd_palette::ESC_CLOSE]),
        ],
        InputMode::Help => vec![hint_from_row(&HELP_ROWS[idx::help::CLOSE])],
        InputMode::ConfirmDialog => vec![],
        InputMode::SqlModal => sql_modal_footer_hints(state, services),
        InputMode::ConnectionSetup => vec![
            hint_from_binding(&CONNECTION_SETUP_KEYS[idx::conn_setup::SAVE]),
            hint_from_binding(&CONNECTION_SETUP_KEYS[idx::conn_setup::TAB_NEXT]),
            hint_from_binding(&CONNECTION_SETUP_KEYS[idx::conn_setup::TAB_PREV]),
            hint_from_binding(&CONNECTION_SETUP_KEYS[idx::conn_setup::ESC_CANCEL]),
        ],
        InputMode::ConnectionError => {
            let first = if state.session.is_service_connection() {
                hint_from_row(&CONNECTION_ERROR_ROWS[idx::conn_error::RETRY])
            } else {
                hint_from_row(&CONNECTION_ERROR_ROWS[idx::conn_error::EDIT])
            };
            vec![
                first,
                hint_from_row(&CONNECTION_ERROR_ROWS[idx::conn_error::SWITCH]),
                hint_from_row(&CONNECTION_ERROR_ROWS[idx::conn_error::DETAILS]),
                hint_from_row(&CONNECTION_ERROR_ROWS[idx::conn_error::COPY]),
                hint_from_row(&CONNECTION_ERROR_ROWS[idx::conn_error::ESC_CLOSE]),
            ]
        }
        InputMode::ErTablePicker => vec![
            hint_from_row(&ER_PICKER_ROWS[idx::er_picker::ENTER_GENERATE]),
            hint_from_row(&ER_PICKER_ROWS[idx::er_picker::SELECT]),
            hint_from_row(&ER_PICKER_ROWS[idx::er_picker::SELECT_ALL]),
            hint_from_row(&ER_PICKER_ROWS[idx::er_picker::TYPE_FILTER]),
            hint_from_row(&ER_PICKER_ROWS[idx::er_picker::ESC_CLOSE]),
        ],
        InputMode::QueryHistoryPicker => vec![
            hint_from_row(&QUERY_HISTORY_PICKER_ROWS[idx::qh_picker::ENTER_SELECT]),
            hint_from_row(&QUERY_HISTORY_PICKER_ROWS[idx::qh_picker::TYPE_FILTER]),
            hint_from_row(&QUERY_HISTORY_PICKER_ROWS[idx::qh_picker::ESC_CLOSE]),
        ],
        InputMode::JsonbDetail => {
            if state.jsonb_detail.search().active {
                vec![
                    hint_from_binding(&JSONB_SEARCH_KEYS[idx::jsonb_search::TYPE_SEARCH]),
                    hint_from_binding(&JSONB_SEARCH_KEYS[idx::jsonb_search::CONFIRM]),
                    hint_from_binding(&JSONB_SEARCH_KEYS[idx::jsonb_search::CANCEL]),
                ]
            } else {
                vec![
                    hint_from_row(&JSONB_DETAIL_ROWS[idx::jsonb_detail::YANK]),
                    hint_from_row(&JSONB_DETAIL_ROWS[idx::jsonb_detail::INSERT]),
                    hint_from_row(&JSONB_DETAIL_ROWS[idx::jsonb_detail::SEARCH]),
                    hint_from_row(&JSONB_DETAIL_ROWS[idx::jsonb_detail::NEXT_PREV]),
                    hint_from_row(&JSONB_DETAIL_ROWS[idx::jsonb_detail::MOVE]),
                    hint_from_row(&JSONB_DETAIL_ROWS[idx::jsonb_detail::CLOSE]),
                ]
            }
        }
        InputMode::JsonbEdit => vec![
            hint_from_row(&JSONB_EDIT_ROWS[idx::jsonb_edit::ESC_NORMAL]),
            hint_from_row(&JSONB_EDIT_ROWS[idx::jsonb_edit::MOVE]),
            hint_from_row(&JSONB_EDIT_ROWS[idx::jsonb_edit::HOME_END]),
        ],
        InputMode::ConnectionSelector => connection_selector_footer_hints(state),
    }
}

pub fn connection_selector_hint_string(state: &AppState) -> String {
    join_hint_text(&connection_selector_footer_hints(state), false)
}

pub fn sql_modal_border_hint(
    state: &AppState,
    active_tab: crate::model::sql_editor::modal::SqlModalTab,
    services: &AppServices,
) -> String {
    let compare_can_yank = state.explain.left.is_some() && state.explain.right.is_some();

    if matches!(state.sql_modal.status(), SqlModalStatus::Editing) {
        let mut hints = vec![
            hint_from_binding(&SQL_MODAL_KEYS[idx::sql_modal::RUN]),
            hint_from_binding(&SQL_MODAL_KEYS[idx::sql_modal::CLEAR]),
            hint_from_binding(&SQL_MODAL_KEYS[idx::sql_modal::QUERY_HISTORY]),
            hint_from_binding(&SQL_MODAL_KEYS[idx::sql_modal::ESC_NORMAL]),
        ];
        if services.db_capabilities.supports_explain() {
            hints.insert(
                1,
                hint_from_binding(&SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::EXPLAIN]),
            );
        }
        return join_hint_text(&hints, true);
    }

    match active_tab {
        crate::model::sql_editor::modal::SqlModalTab::Sql
            if services.db_capabilities.supported_sql_modal_tabs().len() == 1 =>
        {
            join_hint_text(
                &[
                    hint_from_binding(&SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::RUN]),
                    hint_from_binding(&SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::ENTER_INSERT]),
                    hint_from_binding(&SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::CLOSE]),
                ],
                true,
            )
        }
        crate::model::sql_editor::modal::SqlModalTab::Plan => join_hint_text(
            &[
                hint_from_binding(&SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::YANK]),
                Hint {
                    key: "Tab/⇧Tab",
                    description: SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::TAB].as_hint().1,
                },
                hint_from_binding(&SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::CLOSE]),
            ],
            true,
        ),
        crate::model::sql_editor::modal::SqlModalTab::Compare if compare_can_yank => {
            join_hint_text(
                &[
                    hint_from_binding(&SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::EDIT_QUERY]),
                    hint_from_binding(&SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::YANK]),
                    Hint {
                        key: "Tab/⇧Tab",
                        description: SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::TAB]
                            .as_hint()
                            .1,
                    },
                    hint_from_binding(&SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::CLOSE]),
                ],
                true,
            )
        }
        crate::model::sql_editor::modal::SqlModalTab::Compare => join_hint_text(
            &[
                hint_from_binding(&SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::EDIT_QUERY]),
                Hint {
                    key: "Tab/⇧Tab",
                    description: SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::TAB]
                        .as_hint()
                        .1,
                },
                hint_from_binding(&SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::CLOSE]),
            ],
            true,
        ),
        crate::model::sql_editor::modal::SqlModalTab::Sql => join_hint_text(
            &[
                hint_from_binding(&SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::RUN]),
                hint_from_binding(&SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::ENTER_INSERT]),
                Hint {
                    key: "Tab/⇧Tab",
                    description: SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::TAB].as_hint().1,
                },
                hint_from_binding(&SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::CLOSE]),
            ],
            true,
        ),
    }
}

fn join_hint_text(hints: &[Hint], bordered: bool) -> String {
    let separator = if bordered { " │ " } else { "  " };
    let parts: Vec<String> = hints
        .iter()
        .map(|hint| {
            if bordered {
                format!("{}: {}", hint.key, hint.description)
            } else {
                format!("{} {}", hint.key, hint.description)
            }
        })
        .collect();
    format!(" {} ", parts.join(separator))
}

fn sql_modal_footer_hints(state: &AppState, services: &AppServices) -> Vec<Hint> {
    if matches!(
        state.sql_modal.status(),
        SqlModalStatus::ConfirmingHigh { .. }
    ) {
        return vec![hint_from_binding(
            &SQL_MODAL_CONFIRMING_KEYS[idx::sql_modal_confirming::CANCEL_CONFIRM],
        )];
    }
    if matches!(
        state.sql_modal.status(),
        SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error
    ) {
        return vec![];
    }

    let mut hints = vec![
        hint_from_binding(&SQL_MODAL_KEYS[idx::sql_modal::RUN]),
        hint_from_binding(&SQL_MODAL_KEYS[idx::sql_modal::MOVE]),
        hint_from_binding(&SQL_MODAL_KEYS[idx::sql_modal::ESC_NORMAL]),
    ];
    if services.db_capabilities.supports_explain()
        && state.sql_modal.status() == &SqlModalStatus::Editing
    {
        hints.insert(
            1,
            hint_from_binding(&SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::EXPLAIN]),
        );
    }
    hints
}

fn connection_selector_footer_hints(state: &AppState) -> Vec<Hint> {
    use crate::model::connection::list::is_service_selected;
    use idx::connection_selector as cs;

    let is_service_selected = is_service_selected(
        state.connection_list_items(),
        state.ui.connection_list_selected,
    );
    let mut hints = vec![
        hint_from_row(&CONNECTION_SELECTOR_ROWS[cs::CONFIRM]),
        hint_from_row(&CONNECTION_SELECTOR_ROWS[cs::NEW]),
    ];
    if !is_service_selected {
        hints.push(hint_from_row(&CONNECTION_SELECTOR_ROWS[cs::EDIT]));
        hints.push(hint_from_row(&CONNECTION_SELECTOR_ROWS[cs::DELETE]));
    }
    hints.push(hint_from_row(&CONNECTION_SELECTOR_ROWS[cs::CLOSE]));
    hints
}

fn normal_mode_footer_hints(state: &AppState, services: &AppServices) -> Vec<Hint> {
    if state.query.is_history_mode() {
        return vec![
            hint_from_binding(&HISTORY_KEYS[idx::history::NAV]),
            hint_from_binding(&GLOBAL_KEYS[idx::global::HELP]),
            hint_from_binding(&HISTORY_KEYS[idx::history::EXIT]),
        ];
    }

    let result_navigation =
        state.ui.is_focus_mode() || state.ui.focused_pane == FocusedPane::Result;
    let nav_mode = state.result_interaction.selection().mode();

    if result_navigation && nav_mode == ResultNavMode::CellActive {
        if state.result_interaction.cell_edit().has_pending_draft() {
            return vec![
                hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::EDIT]),
                hint_from_binding(&CELL_EDIT_KEYS[idx::cell_edit::WRITE]),
                hint_from_binding(&GLOBAL_KEYS[idx::global::HELP]),
                hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::DRAFT_DISCARD]),
                hint_from_binding(&GLOBAL_KEYS[idx::global::QUIT]),
            ];
        }
        if state.result_interaction.staged_delete_rows().is_empty() {
            return vec![
                hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::EDIT]),
                hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::YANK]),
                hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::ROW_YANK]),
                hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::STAGE_DELETE]),
                hint_from_binding(&GLOBAL_KEYS[idx::global::HELP]),
                hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::ESC_BACK]),
                hint_from_binding(&GLOBAL_KEYS[idx::global::QUIT]),
            ];
        }
        return vec![
            hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::STAGE_DELETE]),
            hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::UNSTAGE_DELETE]),
            hint_from_binding(&CELL_EDIT_KEYS[idx::cell_edit::WRITE]),
            hint_from_binding(&GLOBAL_KEYS[idx::global::HELP]),
            hint_from_binding(&RESULT_ACTIVE_KEYS[idx::result_active::ESC_BACK]),
            hint_from_binding(&GLOBAL_KEYS[idx::global::QUIT]),
        ];
    }

    if state.ui.is_focus_mode() {
        let mut hints = vec![hint_from_binding(
            &RESULT_ACTIVE_KEYS[idx::result_active::ENTER_DEEPEN],
        )];
        if !state.result_interaction.staged_delete_rows().is_empty() {
            hints.push(hint_from_binding(
                &RESULT_ACTIVE_KEYS[idx::result_active::UNSTAGE_DELETE],
            ));
            hints.push(hint_from_binding(&CELL_EDIT_KEYS[idx::cell_edit::WRITE]));
        }
        if state.can_request_csv_export() {
            hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::CSV_EXPORT]));
        }
        if state.query.can_paginate_visible_result() {
            hints.push(hint_from_binding(
                &FOOTER_NAV_KEYS[idx::footer_nav::PAGE_NAV],
            ));
        }
        hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::HELP]));
        hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::EXIT_FOCUS]));
        hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::QUIT]));
        return hints;
    }

    let active_inspector_tab = services
        .db_capabilities
        .normalize_inspector_tab(state.ui.inspector_tab);
    let mut hints = vec![
        hint_from_binding(&GLOBAL_KEYS[idx::global::RELOAD]),
        hint_from_binding(&GLOBAL_KEYS[idx::global::SQL]),
        hint_from_binding(&GLOBAL_KEYS[idx::global::ER_DIAGRAM]),
    ];
    if state.ui.focused_pane == FocusedPane::Explorer {
        hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::CONNECTIONS]));
    }
    hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::TABLE_PICKER]));
    hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::QUERY_HISTORY]));
    if state.connection_error.error_info.is_some() {
        hints.push(hint_from_binding(&OVERLAY_KEYS[idx::overlay::ERROR_OPEN]));
    }
    if state.session.read_only {
        hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::EXIT_READ_ONLY]));
    } else {
        hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::READ_ONLY]));
    }
    hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::FOCUS]));
    if state.can_request_csv_export() {
        hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::CSV_EXPORT]));
    }
    if state.ui.focused_pane == FocusedPane::Inspector && active_inspector_tab == InspectorTab::Ddl
    {
        hints.push(hint_from_binding(
            &INSPECTOR_DDL_KEYS[idx::inspector_ddl::YANK],
        ));
    }
    if state.ui.focused_pane == FocusedPane::Result {
        hints.push(hint_from_binding(
            &RESULT_ACTIVE_KEYS[idx::result_active::ENTER_DEEPEN],
        ));
        if !state.result_interaction.staged_delete_rows().is_empty() {
            hints.push(hint_from_binding(
                &RESULT_ACTIVE_KEYS[idx::result_active::UNSTAGE_DELETE],
            ));
            hints.push(hint_from_binding(&CELL_EDIT_KEYS[idx::cell_edit::WRITE]));
        }
        if state.query.can_paginate_visible_result() {
            hints.push(hint_from_binding(
                &FOOTER_NAV_KEYS[idx::footer_nav::PAGE_NAV],
            ));
        }
    }
    if state.ui.focused_pane == FocusedPane::Inspector
        && services.db_capabilities.supported_inspector_tabs().len() > 1
    {
        hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::INSPECTOR_TABS]));
    }
    hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::HELP]));
    hints.push(hint_from_binding(&GLOBAL_KEYS[idx::global::QUIT]));
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::db_capabilities::DbCapabilities;

    fn inspector_state() -> AppState {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::Normal);
        state.ui.focused_pane = FocusedPane::Inspector;
        state
    }

    #[test]
    fn help_sections_include_global_keys() {
        let sections = help_sections();
        assert_eq!(
            sections.first().map(|section| section.title),
            Some("Global Keys")
        );
        assert!(!sections.first().unwrap().entries.is_empty());
    }

    #[test]
    fn er_waiting_status_text_is_present_only_while_waiting() {
        let mut state = AppState::new("test".to_string());
        assert!(footer_status_text(&state, 0).is_none());

        state.er_preparation.status = ErStatus::Waiting;
        state.er_preparation.total_tables = 10;
        let text = footer_status_text(&state, 0).unwrap();
        assert!(text.contains("Preparing ER"));
    }

    #[test]
    fn inspector_tabs_footer_hint_tracks_supported_tab_count() {
        let state = inspector_state();
        let mut services = AppServices::stub();
        let inspector_tabs_description = GLOBAL_KEYS[idx::global::INSPECTOR_TABS].description;
        services.db_capabilities = DbCapabilities::new(true, vec![InspectorTab::Info]);
        let hidden = footer_hints(&state, &services);
        assert!(
            !hidden
                .iter()
                .any(|hint| hint.description == inspector_tabs_description)
        );

        services.db_capabilities =
            DbCapabilities::new(true, vec![InspectorTab::Info, InspectorTab::Columns]);
        let visible = footer_hints(&state, &services);
        assert!(
            visible
                .iter()
                .any(|hint| hint.description == inspector_tabs_description)
        );
    }
}
