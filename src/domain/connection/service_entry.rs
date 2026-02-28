use super::ConnectionId;

/// Prefix used to construct a ConnectionId for service-based connections.
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
    /// Build the DSN string for psql (e.g. `service=mydb`).
    pub fn to_dsn(&self) -> String {
        format!("service={}", self.service_name)
    }

    /// Build a deterministic ConnectionId for this service entry.
    pub fn connection_id(&self) -> ConnectionId {
        ConnectionId::from_string(format!("{}{}", SERVICE_ID_PREFIX, self.service_name))
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
}
