use crate::app::model::app_state::AppState;
use crate::app::model::shared::settings::{ErBrowserChoice, SettingsSection};
use crate::app::update::input::keybindings::{SETTINGS_ROWS, idx};
use crate::primitives::molecules::FooterHintBar;

const EDIT_DONE_HINT: (&str, &str) = ("Esc", "Done");
const EDIT_TYPE_HINT: (&str, &str) = ("Type", "Browser");

pub fn settings_hints(state: &AppState) -> Vec<(&'static str, &'static str)> {
    if state.settings.is_editing_custom_er_browser() {
        return vec![
            SETTINGS_ROWS[idx::settings::APPLY].as_hint(),
            EDIT_DONE_HINT,
            EDIT_TYPE_HINT,
        ];
    }

    let mut hints = vec![
        SETTINGS_ROWS[idx::settings::APPLY].as_hint(),
        SETTINGS_ROWS[idx::settings::SELECT].as_hint(),
    ];
    if state.settings.section() == SettingsSection::ErDiagram
        && state.settings.selected_er_browser_choice() == ErBrowserChoice::Custom
    {
        hints.push(SETTINGS_ROWS[idx::settings::EDIT].as_hint());
    }
    hints.push(SETTINGS_ROWS[idx::settings::SECTION].as_hint());
    hints.push(SETTINGS_ROWS[idx::settings::CANCEL].as_hint());
    hints
}

pub fn settings_modal_hint_bar(state: &AppState) -> FooterHintBar {
    FooterHintBar::new(settings_hints(state))
        .without_item(SETTINGS_ROWS[idx::settings::SELECT].key_short)
}
