#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub default: Option<String>,
    pub attributes: ColumnAttributes,
    pub comment: Option<String>,
    pub ordinal_position: i32,
    pub character_set_name: Option<String>,
    pub collation_name: Option<String>,
    pub generation_expression: Option<String>,
    pub generation_kind: Option<ColumnGenerationKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnGenerationKind {
    Virtual,
    Stored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColumnAttributes(u8);

impl ColumnAttributes {
    pub const NULLABLE: Self = Self(0b00_0001);
    pub const PRIMARY_KEY: Self = Self(0b00_0010);
    pub const UNIQUE: Self = Self(0b00_0100);
    pub const READ_ONLY: Self = Self(0b00_1000);
    pub const HIDDEN: Self = Self(0b01_0000);
    pub const GENERATED: Self = Self(0b10_0000);

    pub const fn empty() -> Self {
        Self(0)
    }

    /// Builds attributes from raw boolean fields at parser or test-helper boundaries.
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
    pub const fn is_nullable(&self) -> bool {
        self.attributes.contains(ColumnAttributes::NULLABLE)
    }

    pub const fn is_primary_key(&self) -> bool {
        self.attributes.contains(ColumnAttributes::PRIMARY_KEY)
    }

    pub const fn is_unique(&self) -> bool {
        self.attributes.contains(ColumnAttributes::UNIQUE)
    }

    pub const fn is_read_only(&self) -> bool {
        self.attributes.contains(ColumnAttributes::READ_ONLY)
    }

    pub const fn is_hidden(&self) -> bool {
        self.attributes.contains(ColumnAttributes::HIDDEN)
    }

    pub const fn is_generated(&self) -> bool {
        self.attributes.contains(ColumnAttributes::GENERATED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod attributes {
        use super::*;

        #[rstest]
        #[case(false, false, false, false, false, false)]
        #[case(true, false, false, true, false, false)]
        #[case(false, true, false, false, true, false)]
        #[case(false, false, true, false, false, true)]
        #[case(true, true, true, true, true, true)]
        fn from_parts_sets_expected_flags(
            #[case] nullable: bool,
            #[case] primary_key: bool,
            #[case] unique: bool,
            #[case] expected_nullable: bool,
            #[case] expected_primary_key: bool,
            #[case] expected_unique: bool,
        ) {
            let attributes = ColumnAttributes::from_parts(nullable, primary_key, unique);

            assert_eq!(
                attributes.contains(ColumnAttributes::NULLABLE),
                expected_nullable
            );
            assert_eq!(
                attributes.contains(ColumnAttributes::PRIMARY_KEY),
                expected_primary_key
            );
            assert_eq!(
                attributes.contains(ColumnAttributes::UNIQUE),
                expected_unique
            );
        }

        #[test]
        fn bitor_combines_flags() {
            let attributes = ColumnAttributes::NULLABLE | ColumnAttributes::PRIMARY_KEY;

            assert_eq!(attributes, ColumnAttributes::from_parts(true, true, false));
        }
    }
}
