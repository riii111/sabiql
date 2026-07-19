use super::keybindings::{KeyBinding, global};
use crate::model::shared::engine_feature_profile::EngineFeatureProfile;
use crate::model::shared::settings::KeymapPreset;
use crate::update::action::{Action, ModalKind};

// Deliberate opt-in list in display order — not derived from GLOBAL_KEYS, so an
// entry never appears in the palette by accident. A test forces every global
// key to be classified as included here or explicitly excluded.
const PALETTE_COMMANDS: &[KeyBinding] = &[
    global::QUIT,
    global::HELP,
    global::TABLE_PICKER,
    global::SETTINGS,
    global::FOCUS,
    global::RELOAD,
    global::SQL,
    global::ER_DIAGRAM,
    global::CONNECTIONS,
    global::SQLITE_DIAGNOSTICS,
    global::CSV_EXPORT,
    global::READ_ONLY,
    global::EXIT_READ_ONLY,
    global::QUERY_HISTORY,
];

const IDE_PALETTE_COMMANDS: &[KeyBinding] = &[
    global::QUIT,
    global::HELP,
    global::TABLE_PICKER_IDE,
    global::SETTINGS,
    global::FOCUS,
    global::RELOAD,
    global::SQL,
    global::ER_DIAGRAM,
    global::CONNECTIONS,
    global::SQLITE_DIAGNOSTICS_IDE,
    global::CSV_EXPORT_IDE,
    global::READ_ONLY_IDE,
    global::EXIT_READ_ONLY_IDE,
    global::QUERY_HISTORY_IDE,
];

fn palette_commands_for(preset: KeymapPreset) -> &'static [KeyBinding] {
    match preset {
        KeymapPreset::Default => PALETTE_COMMANDS,
        KeymapPreset::Ide => IDE_PALETTE_COMMANDS,
    }
}

pub fn palette_command_count(
    preset: KeymapPreset,
    engine_feature_profile: &EngineFeatureProfile,
) -> usize {
    palette_commands(preset, engine_feature_profile).count()
}

pub fn palette_action_for_index(
    index: usize,
    preset: KeymapPreset,
    engine_feature_profile: &EngineFeatureProfile,
) -> Action {
    palette_commands(preset, engine_feature_profile)
        .nth(index)
        .map_or(Action::None, |kb| kb.action.clone())
}

pub fn palette_commands(
    preset: KeymapPreset,
    engine_feature_profile: &EngineFeatureProfile,
) -> impl Iterator<Item = &'static KeyBinding> {
    palette_commands_for(preset)
        .iter()
        .filter(|kb| palette_command_supported(kb, engine_feature_profile))
}

fn palette_command_supported(
    kb: &KeyBinding,
    engine_feature_profile: &EngineFeatureProfile,
) -> bool {
    !matches!(
        kb.action,
        Action::OpenModal(ModalKind::ErTablePicker) if !engine_feature_profile.supports_er_diagram()
    ) && !matches!(
        kb.action,
        Action::OpenModal(ModalKind::SqliteDiagnostics)
            if !engine_feature_profile.supports_sqlite_diagnostics()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::input::keybindings::{
        GLOBAL_KEYS, IDE_GLOBAL_KEYS, same_payload_free_action,
    };

    // Global keys deliberately kept out of the palette:
    // - COMMAND_LINE: command-line mode is a separate entry mechanism
    // - COMMAND_PALETTE: the palette itself
    // - EXIT_FOCUS: duplicate of FOCUS (same key, context-dependent label)
    // - PANE_SWITCH / INSPECTOR_TABS: Action::None — not executable
    const EXCLUDED_FROM_PALETTE: &[KeyBinding] = &[
        global::COMMAND_LINE,
        global::COMMAND_PALETTE,
        global::EXIT_FOCUS,
        global::PANE_SWITCH,
        global::INSPECTOR_TABS,
    ];

    const IDE_EXCLUDED_FROM_PALETTE: &[KeyBinding] = &[
        global::COMMAND_LINE,
        global::COMMAND_PALETTE_IDE,
        global::EXIT_FOCUS,
        global::PANE_SWITCH,
        global::INSPECTOR_TABS,
    ];

    // Compare the full structure: distinct global keys may share footer display
    // strings, and display-only matching would mark an unclassified newcomer as
    // already classified.
    fn same_entry(a: &KeyBinding, b: &KeyBinding) -> bool {
        a.key_short == b.key_short
            && a.key == b.key
            && a.desc_short == b.desc_short
            && a.description == b.description
            && a.combos == b.combos
            && same_payload_free_action(&a.action, &b.action)
    }

    #[test]
    fn every_global_key_is_classified_for_palette() {
        check_palette_classification(GLOBAL_KEYS, PALETTE_COMMANDS, EXCLUDED_FROM_PALETTE);
        check_palette_classification(
            IDE_GLOBAL_KEYS,
            IDE_PALETTE_COMMANDS,
            IDE_EXCLUDED_FROM_PALETTE,
        );
    }

    fn check_palette_classification(
        global_keys: &[KeyBinding],
        palette_commands: &[KeyBinding],
        excluded_from_palette: &[KeyBinding],
    ) {
        for kb in global_keys {
            let included = palette_commands
                .iter()
                .filter(|p| same_entry(p, kb))
                .count();
            let excluded = excluded_from_palette
                .iter()
                .filter(|e| same_entry(e, kb))
                .count();

            assert_eq!(
                included + excluded,
                1,
                "GLOBAL_KEYS entry '{}' ({}) must appear exactly once across \
                 PALETTE_COMMANDS and EXCLUDED_FROM_PALETTE",
                kb.key,
                kb.desc_short,
            );
        }
    }

    #[test]
    fn palette_commands_contains_no_none_actions() {
        let engine_feature_profile = EngineFeatureProfile::postgres_like();
        for preset in [KeymapPreset::Default, KeymapPreset::Ide] {
            let none_entries: Vec<_> = palette_commands(preset, &engine_feature_profile)
                .filter(|kb| matches!(kb.action, Action::None))
                .collect();

            assert!(
                none_entries.is_empty(),
                "palette_commands({preset:?}) must not contain Action::None entries: {:?}",
                none_entries.iter().map(|kb| kb.key).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn sqlite_palette_omits_er_diagram_command() {
        let commands =
            palette_commands(KeymapPreset::Default, &EngineFeatureProfile::sqlite_like())
                .collect::<Vec<_>>();

        assert!(
            !commands
                .iter()
                .any(|kb| matches!(kb.action, Action::OpenModal(ModalKind::ErTablePicker)))
        );
        assert!(
            commands
                .iter()
                .any(|kb| matches!(kb.action, Action::OpenModal(ModalKind::SqliteDiagnostics)))
        );
    }

    #[test]
    fn postgres_palette_omits_sqlite_diagnostics_command() {
        let commands = palette_commands(
            KeymapPreset::Default,
            &EngineFeatureProfile::postgres_like(),
        )
        .collect::<Vec<_>>();

        assert!(
            !commands
                .iter()
                .any(|kb| matches!(kb.action, Action::OpenModal(ModalKind::SqliteDiagnostics)))
        );
    }
}
