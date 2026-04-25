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
