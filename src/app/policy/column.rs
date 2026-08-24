use crate::domain::{Column, ColumnGenerationKind};

pub const fn column_generation_kind_label(kind: ColumnGenerationKind) -> &'static str {
    match kind {
        ColumnGenerationKind::Virtual => "VIRTUAL",
        ColumnGenerationKind::Stored => "STORED",
    }
}

pub const fn column_read_only_reason(column: &Column) -> Option<&'static str> {
    if column.is_generated() {
        Some("generated")
    } else if column.is_hidden() {
        Some("hidden")
    } else if column.is_read_only() {
        Some("read-only")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{Column, ColumnAttributes, ColumnGenerationKind};
    use rstest::rstest;

    use super::{column_generation_kind_label, column_read_only_reason};

    #[rstest]
    #[case(ColumnGenerationKind::Virtual, "VIRTUAL")]
    #[case(ColumnGenerationKind::Stored, "STORED")]
    fn generation_kind_label_describes_storage_mode(
        #[case] kind: ColumnGenerationKind,
        #[case] expected: &str,
    ) {
        assert_eq!(column_generation_kind_label(kind), expected);
    }

    #[rstest]
    #[case(true, false, false, Some("read-only"))]
    #[case(true, true, false, Some("hidden"))]
    #[case(true, false, true, Some("generated"))]
    #[case(true, true, true, Some("generated"))]
    #[case(false, false, false, None)]
    fn column_flags_report_read_only_reason(
        #[case] read_only: bool,
        #[case] hidden: bool,
        #[case] generated: bool,
        #[case] expected: Option<&str>,
    ) {
        let mut attributes = ColumnAttributes::from_parts(true, false, false);
        if read_only {
            attributes = attributes | ColumnAttributes::READ_ONLY;
        }
        if hidden {
            attributes = attributes | ColumnAttributes::HIDDEN;
        }
        if generated {
            attributes = attributes | ColumnAttributes::GENERATED;
        }
        let column = Column {
            name: "col".to_string(),
            data_type: "text".to_string(),
            default: None,
            attributes,
            comment: None,
            ordinal_position: 1,
            character_set_name: None,
            collation_name: None,
            generation_expression: None,
            generation_kind: None,
        };

        assert_eq!(column_read_only_reason(&column), expected);
    }
}
