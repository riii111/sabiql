use super::ConnectionId;

const SERVICE_ID_PREFIX: &str = "service:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEntry {
    pub service_name: String,
    pub host: Option<String>,
    pub dbname: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
}

impl ServiceEntry {
    pub fn to_dsn(&self) -> String {
        format!("service={}", self.service_name)
    }

    pub fn connection_id(&self) -> ConnectionId {
        ConnectionId::from_string(format!("{}{}", SERVICE_ID_PREFIX, self.service_name))
    }

    pub fn display_name(&self) -> &str {
        &self.service_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ServiceEntry {
        ServiceEntry {
            service_name: "mydb".to_string(),
            host: None,
            dbname: None,
            port: None,
            user: None,
        }
    }

    #[test]
    fn to_dsn_formats_correctly() {
        assert_eq!(sample().to_dsn(), "service=mydb");
    }

    #[test]
    fn connection_id_uses_prefix() {
        let id = sample().connection_id();
        assert_eq!(id, ConnectionId::from_string("service:mydb".to_string()));
    }

    #[test]
    fn display_name_returns_service_name() {
        assert_eq!(sample().display_name(), "mydb");
    }
}
