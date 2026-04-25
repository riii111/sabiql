use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub default: Option<String>,
    pub attributes: ColumnAttributes,
    pub comment: Option<String>,
    pub ordinal_position: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColumnAttributes(u8);

impl ColumnAttributes {
    pub const NULLABLE: Self = Self(0b001);
    pub const PRIMARY_KEY: Self = Self(0b010);
    pub const UNIQUE: Self = Self(0b100);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn from_parts(nullable: bool, primary_key: bool, unique: bool) -> Self {
        let mut bits = 0;
        if nullable {
            bits |= Self::NULLABLE.0;
        }
        if primary_key {
            bits |= Self::PRIMARY_KEY.0;
        }
        if unique {
            bits |= Self::UNIQUE.0;
        }
        Self(bits)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl std::ops::BitOr for ColumnAttributes {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl Column {
    pub const fn nullable(&self) -> bool {
        self.attributes.contains(ColumnAttributes::NULLABLE)
    }

    pub const fn is_primary_key(&self) -> bool {
        self.attributes.contains(ColumnAttributes::PRIMARY_KEY)
    }

    pub const fn is_unique(&self) -> bool {
        self.attributes.contains(ColumnAttributes::UNIQUE)
    }

    pub fn type_display(&self) -> String {
        let mut display = self.data_type.clone();
        if !self.nullable() {
            display.push_str(" NOT NULL");
        }
        if let Some(default) = &self.default {
            let _ = write!(display, " DEFAULT {default}");
        }
        display
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn make_column(nullable: bool, default: Option<&str>) -> Column {
        Column {
            name: "col".to_string(),
            data_type: "integer".to_string(),
            default: default.map(ToString::to_string),
            attributes: ColumnAttributes::from_parts(nullable, false, false),
            comment: None,
            ordinal_position: 1,
        }
    }

    mod type_display {
        use super::*;

        #[rstest]
        #[case(true, None, "integer")]
        #[case(false, None, "integer NOT NULL")]
        #[case(true, Some("0"), "integer DEFAULT 0")]
        #[case(false, Some("now()"), "integer NOT NULL DEFAULT now()")]
        fn formats_sql_type(
            #[case] nullable: bool,
            #[case] default: Option<&str>,
            #[case] expected: &str,
        ) {
            let column = make_column(nullable, default);

            assert_eq!(column.type_display(), expected);
        }
    }
}
