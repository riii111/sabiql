use crate::app::model::app_state::AppState;
use crate::app::update::input::keymap::{HintSurface, KeyContext, settings_hints};
use crate::primitives::molecules::FooterHintBar;

pub(crate) fn settings_modal_hint_bar(state: &AppState) -> FooterHintBar {
    let hints = settings_hints(HintSurface::Border, &KeyContext::from_state(state))
        .into_iter()
        .map(|hint| (hint.key, hint.short));
    FooterHintBar::new(hints)
}
