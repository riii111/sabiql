use crate::domain::connection::ConnectionEndpoint;

pub trait DsnBuilder: Send + Sync {
    fn build_dsn(&self, endpoint: &ConnectionEndpoint) -> String;
}
