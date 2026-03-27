use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub is_primary_key: bool,
    pub is_unique: bool,
    pub comment: Option<String>,
    pub ordinal_position: i32,
}

impl Column {
    pub fn type_display(&self) -> String {
        let mut display = self.data_type.clone();
        if !self.nullable {
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
            nullable,
            default: default.map(ToString::to_string),
            is_primary_key: false,
            is_unique: false,
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
        fn returns_expected_format(
            #[case] nullable: bool,
            #[case] default: Option<&str>,
            #[case] expected: &str,
        ) {
            let column = make_column(nullable, default);

            assert_eq!(column.type_display(), expected);
        }
    }
}
