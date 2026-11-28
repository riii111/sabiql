use ratatui::text::{Line, Span};

use crate::primitives::atoms::key_text;
use crate::theme::ThemePalette;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FooterHintItem {
    key: String,
    desc: &'static str,
}

impl FooterHintItem {
    fn new(key: impl Into<String>, desc: &'static str) -> Self {
        Self {
            key: key.into(),
            desc,
        }
    }

    fn width(&self) -> usize {
        self.key.chars().count() + ": ".chars().count() + self.desc.chars().count()
    }
}

impl From<(String, &'static str)> for FooterHintItem {
    fn from((key, desc): (String, &'static str)) -> Self {
        Self::new(key, desc)
    }
}

impl From<(&'static str, &'static str)> for FooterHintItem {
    fn from((key, desc): (&'static str, &'static str)) -> Self {
        Self::new(key, desc)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FooterHintBar {
    prefix: Option<String>,
    items: Vec<FooterHintItem>,
}

impl FooterHintBar {
    pub(crate) fn new(items: impl IntoIterator<Item = impl Into<FooterHintItem>>) -> Self {
        Self {
            prefix: None,
            items: items.into_iter().map(Into::into).collect(),
        }
    }

    pub(crate) fn message(message: impl Into<String>) -> Self {
        Self {
            prefix: Some(message.into()),
            items: Vec::new(),
        }
    }

    pub(crate) fn with_prefix(
        prefix: impl Into<String>,
        items: impl IntoIterator<Item = impl Into<FooterHintItem>>,
    ) -> Self {
        Self {
            prefix: Some(prefix.into()),
            items: items.into_iter().map(Into::into).collect(),
        }
    }

    pub(crate) fn line(&self, theme: &ThemePalette) -> Line<'static> {
        let mut spans = Vec::new();

        spans.push(Span::styled(" ", theme.modal_hint_style()));
        if let Some(prefix) = &self.prefix {
            spans.push(Span::styled(prefix.clone(), theme.modal_hint_style()));
            if !self.items.is_empty() {
                spans.push(Span::styled(" │ ", theme.modal_hint_style()));
            }
        }

        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" │ ", theme.modal_hint_style()));
            }
            spans.push(key_text(&item.key, theme));
            spans.push(Span::styled(
                format!(": {}", item.desc),
                theme.modal_hint_style(),
            ));
        }
        spans.push(Span::styled(" ", theme.modal_hint_style()));

        Line::from(spans)
    }

    pub(crate) fn width(&self) -> u16 {
        let mut width = 2usize;
        if let Some(prefix) = &self.prefix {
            width += prefix.chars().count();
            if !self.items.is_empty() {
                width += " │ ".chars().count();
            }
        }

        width += self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| item.width() + if i > 0 { " │ ".chars().count() } else { 0 })
            .sum::<usize>();

        width as u16
    }
}
