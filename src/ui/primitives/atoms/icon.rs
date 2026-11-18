use crate::app::model::shared::settings::IconMode;
use crate::domain::{DatabaseType, TableKind};

#[derive(Clone, Copy)]
pub enum Icon {
    Project,
    Table,
    View,
    Column,
    PrimaryKey,
    ForeignKey,
    Postgres,
    MySql,
    Sqlite,
    Quit,
    Help,
    Settings,
    Focus,
    Reload,
    Query,
    Diagram,
    Connections,
    Diagnostics,
    Export,
    ReadOnly,
    History,
}

impl Icon {
    pub fn prefix(self, mode: IconMode) -> &'static str {
        let (nerd_font, unicode, ascii) = match self {
            Self::Project => ("\u{f07b} ", "⌂ ", "./ "),
            Self::Table => ("\u{f0ce} ", "▦ ", "T "),
            Self::View => ("\u{f06e} ", "◇ ", "V "),
            Self::Column => ("\u{f0db} ", "│ ", "C "),
            Self::PrimaryKey => ("\u{f084} ", "◆ ", "P "),
            Self::ForeignKey => ("\u{f0c1} ", "↗ ", "F "),
            Self::Postgres => ("\u{e76e} ", "Pg ", "Pg "),
            Self::MySql => ("\u{e704} ", "My ", "My "),
            Self::Sqlite => ("\u{e7c4} ", "Sq ", "Sq "),
            Self::Quit => ("\u{f08b} ", "× ", "x "),
            Self::Help => ("\u{f059} ", "? ", "? "),
            Self::Settings => ("\u{f013} ", "⚙ ", "* "),
            Self::Focus => ("\u{f065} ", "□ ", "+ "),
            Self::Reload => ("\u{f021} ", "↻ ", "~ "),
            Self::Query => ("\u{f120} ", "▷ ", "> "),
            Self::Diagram => ("\u{f0e8} ", "⑂ ", "& "),
            Self::Connections => ("\u{f1e6} ", "⇄ ", "= "),
            Self::Diagnostics => ("\u{f0f1} ", "+ ", "+ "),
            Self::Export => ("\u{f019} ", "↓ ", "v "),
            Self::ReadOnly => ("\u{f023} ", "⊘ ", "! "),
            Self::History => ("\u{f1da} ", "◷ ", "@ "),
        };
        match mode {
            IconMode::NerdFont => nerd_font,
            IconMode::Unicode => unicode,
            IconMode::Ascii => ascii,
            IconMode::Off => "",
        }
    }
}

impl From<TableKind> for Icon {
    fn from(kind: TableKind) -> Self {
        match kind {
            TableKind::Table | TableKind::Virtual => Self::Table,
            TableKind::View => Self::View,
        }
    }
}

impl From<DatabaseType> for Icon {
    fn from(engine: DatabaseType) -> Self {
        match engine {
            DatabaseType::PostgreSQL => Self::Postgres,
            DatabaseType::MySQL => Self::MySql,
            DatabaseType::SQLite => Self::Sqlite,
        }
    }
}
