use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub attributes: IndexAttributes,
    pub index_type: IndexType,
    pub definition: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexAttributes(u8);

impl IndexAttributes {
    pub const UNIQUE: Self = Self(0b01);
    pub const PRIMARY: Self = Self(0b10);

    pub const fn empty() -> Self {
        Self(0)
    }

    /// Builds attributes from raw boolean fields at parser or test-helper boundaries.
    pub const fn from_parts(unique: bool, primary: bool) -> Self {
        let mut bits = 0;
        if unique {
            bits |= Self::UNIQUE.0;
        }
        if primary {
            bits |= Self::PRIMARY.0;
        }
        Self(bits)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl std::ops::BitOr for IndexAttributes {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl Index {
    pub const fn is_unique(&self) -> bool {
        self.attributes.contains(IndexAttributes::UNIQUE)
    }

    pub const fn is_primary(&self) -> bool {
        self.attributes.contains(IndexAttributes::PRIMARY)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum IndexType {
    #[default]
    BTree,
    Hash,
    Gist,
    Gin,
    Brin,
    Other(String),
}

impl std::fmt::Display for IndexType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BTree => write!(f, "btree"),
            Self::Hash => write!(f, "hash"),
            Self::Gist => write!(f, "gist"),
            Self::Gin => write!(f, "gin"),
            Self::Brin => write!(f, "brin"),
            Self::Other(s) => write!(f, "{s}"),
        }
    }
}

impl FromStr for IndexType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "btree" => Self::BTree,
            "hash" => Self::Hash,
            "gist" => Self::Gist,
            "gin" => Self::Gin,
            "brin" => Self::Brin,
            _ => Self::Other(s.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn make_index(attributes: IndexAttributes) -> Index {
        Index {
            name: "idx".to_string(),
            columns: vec!["id".to_string()],
            attributes,
            index_type: IndexType::BTree,
            definition: None,
        }
    }

    mod attributes {
        use super::*;

        #[rstest]
        #[case(false, false, false, false)]
        #[case(true, false, true, false)]
        #[case(false, true, false, true)]
        #[case(true, true, true, true)]
        fn from_parts_sets_expected_flags(
            #[case] unique: bool,
            #[case] primary: bool,
            #[case] expected_unique: bool,
            #[case] expected_primary: bool,
        ) {
            let attributes = IndexAttributes::from_parts(unique, primary);

            assert_eq!(
                attributes.contains(IndexAttributes::UNIQUE),
                expected_unique
            );
            assert_eq!(
                attributes.contains(IndexAttributes::PRIMARY),
                expected_primary
            );
        }

        #[test]
        fn bitor_combines_flags() {
            let attributes = IndexAttributes::UNIQUE | IndexAttributes::PRIMARY;

            assert!(attributes.contains(IndexAttributes::UNIQUE));
            assert!(attributes.contains(IndexAttributes::PRIMARY));
        }
    }

    mod index_helpers {
        use super::*;

        #[rstest]
        #[case(IndexAttributes::UNIQUE, true, false)]
        #[case(IndexAttributes::PRIMARY, false, true)]
        #[case(IndexAttributes::UNIQUE | IndexAttributes::PRIMARY, true, true)]
        #[case(IndexAttributes::empty(), false, false)]
        fn report_expected_attribute_state(
            #[case] attributes: IndexAttributes,
            #[case] expected_unique: bool,
            #[case] expected_primary: bool,
        ) {
            let index = make_index(attributes);

            assert_eq!(index.is_unique(), expected_unique);
            assert_eq!(index.is_primary(), expected_primary);
        }
    }

    #[rstest]
    #[case(IndexType::BTree)]
    #[case(IndexType::Hash)]
    #[case(IndexType::Gist)]
    #[case(IndexType::Gin)]
    #[case(IndexType::Brin)]
    #[case(IndexType::Other("custom_am".to_string()))]
    fn display_round_trips(#[case] index_type: IndexType) {
        assert_eq!(
            index_type.to_string().parse::<IndexType>().unwrap(),
            index_type
        );
    }

    #[rstest]
    #[case("btree", IndexType::BTree)]
    #[case("HASH", IndexType::Hash)]
    #[case("gist", IndexType::Gist)]
    #[case("custom_am", IndexType::Other("custom_am".to_string()))]
    fn from_str_parses_known_and_unknown_types(#[case] input: &str, #[case] expected: IndexType) {
        assert_eq!(input.parse::<IndexType>().unwrap(), expected);
    }
}
