use crate::domain::connection::ConnectionProfile;

pub trait DsnBuilder: Send + Sync {
    fn build_dsn(&self, profile: &ConnectionProfile) -> String;
}
