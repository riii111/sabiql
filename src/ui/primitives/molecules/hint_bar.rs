use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::primitives::atoms::{key_chip, key_text};
use crate::theme::ThemePalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FooterHintItem {
    key: &'static str,
    desc: &'static str,
}

impl FooterHintItem {
    pub const fn new(key: &'static str, desc: &'static str) -> Self {
        Self { key, desc }
    }

    fn width(self) -> usize {
        self.key.chars().count() + ": ".chars().count() + self.desc.chars().count()
    }
}

impl From<(&'static str, &'static str)> for FooterHintItem {
    fn from((key, desc): (&'static str, &'static str)) -> Self {
        Self::new(key, desc)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterHintBar {
    prefix: Option<String>,
    items: Vec<FooterHintItem>,
}

impl FooterHintBar {
    pub fn new(items: impl IntoIterator<Item = impl Into<FooterHintItem>>) -> Self {
        Self {
            prefix: None,
            items: items.into_iter().map(Into::into).collect(),
        }
    }

    pub fn message(message: impl Into<String>) -> Self {
        Self {
            prefix: Some(message.into()),
            items: Vec::new(),
        }
    }

    pub fn with_prefix(
        prefix: impl Into<String>,
        items: impl IntoIterator<Item = impl Into<FooterHintItem>>,
    ) -> Self {
        Self {
            prefix: Some(prefix.into()),
            items: items.into_iter().map(Into::into).collect(),
        }
    }

    pub fn without_item(mut self, key: &str) -> Self {
        self.items.retain(|item| item.key != key);
        self
    }

    pub fn line(&self, theme: &ThemePalette) -> Line<'static> {
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
            spans.push(key_text(item.key, theme));
            spans.push(Span::styled(
                format!(": {}", item.desc),
                theme.modal_hint_style(),
            ));
        }
        spans.push(Span::styled(" ", theme.modal_hint_style()));

        Line::from(spans)
    }

    pub fn width(&self) -> u16 {
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

pub fn hint_line(hints: &[(&str, &str)], theme: &ThemePalette) -> Line<'static> {
    let mut spans = Vec::new();

    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(key_text(key, theme));
        spans.push(Span::raw(format!(":{desc}")));
    }

    Line::from(spans)
}

pub fn chip_hint_line(key: &str, desc: &str, theme: &ThemePalette) -> Line<'static> {
    let chip = key_chip(key, theme);
    let padding_len = 15usize.saturating_sub(key.len() + 4);

    Line::from(vec![
        Span::raw("  "),
        chip,
        Span::raw(" ".repeat(padding_len)),
        Span::styled(
            desc.to_string(),
            Style::default().fg(theme.semantic.text.secondary),
        ),
    ])
}
