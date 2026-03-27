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
