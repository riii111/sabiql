use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub index_type: IndexType,
    pub definition: Option<String>,
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
