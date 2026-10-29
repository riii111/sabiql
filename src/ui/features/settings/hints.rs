use crate::app::update::input::keybindings::settings;
use crate::primitives::molecules::FooterHintBar;

pub fn settings_hints() -> Vec<(&'static str, &'static str)> {
    vec![
        settings::APPLY.as_hint(),
        settings::SELECT.as_hint(),
        settings::SECTION.as_hint(),
        settings::CANCEL.as_hint(),
    ]
}

pub fn settings_modal_hint_bar() -> FooterHintBar {
    FooterHintBar::new(settings_hints()).without_item(settings::SELECT.key_short)
}
