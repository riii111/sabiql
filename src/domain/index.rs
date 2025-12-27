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
            IndexType::BTree => write!(f, "btree"),
            IndexType::Hash => write!(f, "hash"),
            IndexType::Gist => write!(f, "gist"),
            IndexType::Gin => write!(f, "gin"),
            IndexType::Brin => write!(f, "brin"),
            IndexType::Other(s) => write!(f, "{}", s),
        }
    }
}
