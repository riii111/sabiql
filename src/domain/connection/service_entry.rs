#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEntry {
    pub service_name: String,
    pub host: Option<String>,
    pub dbname: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
}
