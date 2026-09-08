use std::fmt;

use url::Url;

use crate::domain::{ConnectionId, DatabaseType};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::policy::mask_password;
use crate::ports::outbound::SqlitePathValidator;

use super::cli_sqlite::{
    CliSqliteActivateError, CliSqliteResolveError, CliSqliteTarget, activate_cli_sqlite_connection,
    resolve_cli_sqlite_target,
};

#[derive(Clone, PartialEq, Eq)]
pub struct CliUriTarget {
    id: ConnectionId,
    name: String,
    database_type: DatabaseType,
    database: Option<String>,
    dsn: String,
}

impl fmt::Debug for CliUriTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CliUriTarget")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("database_type", &self.database_type)
            .field("database", &self.database)
            .field("dsn", &mask_password(&self.dsn))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliConnectionTarget {
    SQLite(CliSqliteTarget),
    Uri(CliUriTarget),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliConnectionResolveError {
    #[error("{0}")]
    SQLite(#[from] CliSqliteResolveError),
    #[error(
        "Unsupported connection target; use a SQLite path, sqlite:// DSN, PostgreSQL/MySQL URI, or --connection-env NAME"
    )]
    UnsupportedFormat,
    #[error("Invalid {0} connection URI")]
    InvalidUri(&'static str),
    #[error("Connection environment variable {0} is not set or empty")]
    EnvironmentVariableUnavailable(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CliConnectionActivateError {
    #[error("{0}")]
    SQLite(#[from] CliSqliteActivateError),
}

impl CliUriTarget {
    fn parse(input: &str) -> Result<Self, CliConnectionResolveError> {
        let dsn = input.trim();
        let Some((scheme, remainder)) = dsn.split_once("://") else {
            return Err(CliConnectionResolveError::UnsupportedFormat);
        };
        let (database_type, label) = match scheme {
            "postgres" | "postgresql" => (DatabaseType::PostgreSQL, "PostgreSQL"),
            "mysql" => (DatabaseType::MySQL, "MySQL"),
            _ => return Err(CliConnectionResolveError::UnsupportedFormat),
        };
        if remainder.is_empty() || dsn.chars().any(|character| character == '\0') {
            return Err(CliConnectionResolveError::InvalidUri(label));
        }

        let parsed = Url::parse(dsn).ok();
        let database = (database_type == DatabaseType::MySQL)
            .then(|| {
                parsed.as_ref().and_then(|url| {
                    url.path_segments()
                        .and_then(|mut segments| segments.next())
                        .filter(|segment| !segment.is_empty())
                        .and_then(|segment| urlencoding::decode(segment).ok())
                        .map(std::borrow::Cow::into_owned)
                })
            })
            .flatten();
        let host = parsed
            .as_ref()
            .and_then(Url::host_str)
            .filter(|host| !host.is_empty());
        let name = match (host, database.as_deref()) {
            (Some(host), Some(database)) => format!("{host}/{database}"),
            (Some(host), None) => host.to_string(),
            (None, Some(database)) => database.to_string(),
            (None, None) => label.to_string(),
        };

        Ok(Self {
            id: stable_cli_connection_id(dsn),
            name,
            database_type,
            database,
            dsn: dsn.to_string(),
        })
    }
}

fn stable_cli_connection_id(dsn: &str) -> ConnectionId {
    let identity = redact_uri_passwords(dsn);
    let id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, identity.as_bytes());
    ConnectionId::from_string(format!("cli:{id}"))
}

fn redact_uri_passwords(uri: &str) -> String {
    let Some(scheme_end) = uri.find("://") else {
        return uri.to_string();
    };
    let authority_start = scheme_end + 3;
    let authority_end = uri[authority_start..]
        .find(['/', '?', '#'])
        .map_or(uri.len(), |offset| authority_start + offset);
    let mut ranges = Vec::new();
    let authority = &uri[authority_start..authority_end];
    if let Some(at_offset) = authority.rfind('@') {
        let userinfo_end = authority_start + at_offset;
        if let Some(colon_offset) = authority[..at_offset].find(':') {
            ranges.push((authority_start + colon_offset + 1, userinfo_end));
        }
    }

    if let Some(query_offset) = uri[authority_end..].find('?') {
        let query_start = authority_end + query_offset + 1;
        let query_end = uri[query_start..]
            .find('#')
            .map_or(uri.len(), |offset| query_start + offset);
        let query = &uri[query_start..query_end];
        let mut segment_start = query_start;
        for segment in query.split('&') {
            let segment_end = segment_start + segment.len();
            if let Some(equal_offset) = segment.find('=') {
                let key = &segment[..equal_offset];
                if key.eq_ignore_ascii_case("password") {
                    ranges.push((segment_start + equal_offset + 1, segment_end));
                }
            }
            segment_start = segment_end.saturating_add(1);
        }
    }

    let mut redacted = uri.to_string();
    for (start, end) in ranges.into_iter().rev() {
        redacted.replace_range(start..end, "");
    }
    redacted
}

pub fn resolve_cli_connection_target(
    input: &str,
    validator: &impl SqlitePathValidator,
) -> Result<CliConnectionTarget, CliConnectionResolveError> {
    let input = input.trim();
    if let Some((scheme, _)) = input.split_once("://") {
        return match scheme {
            "postgres" | "postgresql" | "mysql" => {
                Ok(CliConnectionTarget::Uri(CliUriTarget::parse(input)?))
            }
            "sqlite" => Ok(CliConnectionTarget::SQLite(resolve_cli_sqlite_target(
                input, validator,
            )?)),
            _ => Err(CliConnectionResolveError::UnsupportedFormat),
        };
    }
    if input.starts_with("service=") {
        return Err(CliConnectionResolveError::UnsupportedFormat);
    }
    Ok(CliConnectionTarget::SQLite(resolve_cli_sqlite_target(
        input, validator,
    )?))
}

pub fn resolve_cli_connection_env(
    name: &str,
    validator: &impl SqlitePathValidator,
) -> Result<CliConnectionTarget, CliConnectionResolveError> {
    let value = std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliConnectionResolveError::EnvironmentVariableUnavailable(name.into()))?;
    let target = resolve_cli_connection_target(&value, validator)?;
    if matches!(&target, CliConnectionTarget::SQLite(_)) {
        return Err(CliConnectionResolveError::UnsupportedFormat);
    }
    Ok(target)
}

pub fn activate_cli_connection(
    state: &mut AppState,
    target: &CliConnectionTarget,
    validator: &impl SqlitePathValidator,
) -> Result<(), CliConnectionActivateError> {
    match target {
        CliConnectionTarget::SQLite(target) => {
            activate_cli_sqlite_connection(state, target, validator)?;
        }
        CliConnectionTarget::Uri(target) => {
            state.session.activate_cli_ephemeral_connection_with_target(
                &target.id,
                &target.name,
                target.database_type,
                &target.dsn,
                target.database.as_deref(),
            );
            state.modal.set_mode(InputMode::Normal);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SqlitePathError;

    struct AcceptingValidator;

    impl SqlitePathValidator for AcceptingValidator {
        fn validate_database_path(&self, _path: &str) -> Result<(), SqlitePathError> {
            Ok(())
        }

        fn canonicalize_database_path(
            &self,
            path: &str,
        ) -> Result<std::path::PathBuf, SqlitePathError> {
            Ok(path.into())
        }
    }

    #[test]
    fn routes_sqlite_paths_to_existing_target() {
        let target = resolve_cli_connection_target("data.db", &AcceptingValidator).unwrap();

        assert!(matches!(target, CliConnectionTarget::SQLite(_)));
    }

    #[test]
    fn accepts_postgres_uri_schemes() {
        for scheme in ["postgres", "postgresql"] {
            let target = resolve_cli_connection_target(
                &format!("{scheme}://user:secret@localhost/app"),
                &AcceptingValidator,
            )
            .unwrap();

            let CliConnectionTarget::Uri(target) = target else {
                panic!("expected URI target");
            };
            assert_eq!(target.database_type, DatabaseType::PostgreSQL);
            assert_eq!(target.name, "localhost");
            assert!(!format!("{target:?}").contains("secret"));
        }
    }

    #[test]
    fn extracts_mysql_database_without_exposing_credentials() {
        let target = resolve_cli_connection_target(
            "mysql://user:secret@localhost/app?ssl-mode=REQUIRED",
            &AcceptingValidator,
        )
        .unwrap();

        let CliConnectionTarget::Uri(target) = target else {
            panic!("expected URI target");
        };
        assert_eq!(target.database_type, DatabaseType::MySQL);
        assert_eq!(target.database.as_deref(), Some("app"));
        assert_eq!(target.name, "localhost/app");
        assert!(!format!("{target:?}").contains("secret"));
    }

    #[test]
    fn reuses_uri_connection_id_without_password() {
        let first = resolve_cli_connection_target(
            "mysql://user:first-secret@localhost/app",
            &AcceptingValidator,
        )
        .unwrap();
        let second = resolve_cli_connection_target(
            "mysql://user:second-secret@localhost/app",
            &AcceptingValidator,
        )
        .unwrap();

        let (CliConnectionTarget::Uri(first), CliConnectionTarget::Uri(second)) = (first, second)
        else {
            panic!("expected URI targets");
        };
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn rejects_unsupported_input_without_echoing_it() {
        let result = resolve_cli_connection_target(
            "redis://user:secret@example.com/db",
            &AcceptingValidator,
        );

        assert!(matches!(
            result,
            Err(CliConnectionResolveError::UnsupportedFormat)
        ));
        assert!(!result.unwrap_err().to_string().contains("secret"));
    }

    #[test]
    fn activates_mysql_uri_as_an_ephemeral_connection() {
        let target =
            resolve_cli_connection_target("mysql://user:secret@localhost/app", &AcceptingValidator)
                .unwrap();
        let mut state = AppState::new("test".to_string());

        activate_cli_connection(&mut state, &target, &AcceptingValidator).unwrap();

        assert_eq!(
            state.session.active_database_type(),
            Some(DatabaseType::MySQL)
        );
        assert_eq!(state.session.active_database(), Some("app"));
        assert!(state.session.is_ephemeral_connection());
        assert!(state.connections().is_empty());
        assert_eq!(state.input_mode(), InputMode::Normal);
    }
}
