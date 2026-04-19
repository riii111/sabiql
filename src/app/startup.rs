use std::path::PathBuf;

use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::ports::{ConnectionStoreError, ServiceFileError};
use crate::services::AppServices;
use crate::update::action::ConnectionsLoadedPayload;
use crate::update::reducer::reduce;

#[derive(Debug)]
pub enum StartupLoadError {
    VersionMismatch { found: u32, expected: u32 },
}

pub fn initialize_connection_state(
    state: &mut AppState,
    services: &AppServices,
    profiles_result: Result<Vec<crate::domain::connection::ConnectionProfile>, ConnectionStoreError>,
    service_result: Option<Result<(Vec<crate::domain::connection::ServiceEntry>, PathBuf), ServiceFileError>>,
) -> Result<(), StartupLoadError> {
    match profiles_result {
        Ok(profiles) => {
            let (service_entries, service_file_path, service_load_warning) = match service_result {
                Some(Ok((services, path))) if !services.is_empty() => (services, Some(path), None),
                Some(Ok(_)) | Some(Err(ServiceFileError::NotFound(_))) | None => {
                    (Vec::new(), None, None)
                }
                Some(Err(error)) => (Vec::new(), None, Some(error.to_string())),
            };

            let payload = ConnectionsLoadedPayload {
                profiles,
                services: service_entries,
                service_file_path,
                profile_load_warning: None,
                service_load_warning,
            };
            reduce(
                state,
                crate::update::action::Action::ConnectionsLoaded(payload),
                std::time::Instant::now(),
                services,
            );

            if state.connection_list_items().is_empty() {
                state.connection_setup.is_first_run = true;
                state.modal.set_mode(InputMode::ConnectionSetup);
            } else {
                state.connection_setup.is_first_run = false;
                state.modal.set_mode(InputMode::ConnectionSelector);
                state.ui.set_connection_list_selection(Some(0));
            }
            Ok(())
        }
        Err(ConnectionStoreError::VersionMismatch { found, expected }) => {
            Err(StartupLoadError::VersionMismatch { found, expected })
        }
        Err(_) => {
            state.connection_setup.is_first_run = true;
            state.modal.set_mode(InputMode::ConnectionSetup);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::{
        ConnectionId, ConnectionName, ConnectionProfile, ServiceEntry, SslMode,
    };

    fn profile(name: &str) -> ConnectionProfile {
        ConnectionProfile {
            id: ConnectionId::new(),
            name: ConnectionName::new(name).unwrap(),
            host: "localhost".to_string(),
            port: 5432,
            database: "postgres".to_string(),
            username: "postgres".to_string(),
            password: "secret".to_string(),
            ssl_mode: SslMode::Prefer,
        }
    }

    fn service(name: &str) -> ServiceEntry {
        ServiceEntry {
            service_name: name.to_string(),
            host: None,
            dbname: None,
            port: None,
            user: None,
        }
    }

    #[test]
    fn opens_setup_when_no_profiles_or_services_exist() {
        let mut state = AppState::new("test".to_string());

        initialize_connection_state(
            &mut state,
            &crate::services::AppServices::stub(),
            Ok(vec![]),
            Some(Ok((vec![], "/tmp/pg_service.conf".into()))),
        )
        .unwrap();

        assert_eq!(state.input_mode(), InputMode::ConnectionSetup);
        assert!(state.connection_setup.is_first_run);
    }

    #[test]
    fn opens_selector_when_service_entries_exist() {
        let mut state = AppState::new("test".to_string());

        initialize_connection_state(
            &mut state,
            &crate::services::AppServices::stub(),
            Ok(vec![]),
            Some(Ok((vec![service("svc")], "/tmp/pg_service.conf".into()))),
        )
        .unwrap();

        assert_eq!(state.input_mode(), InputMode::ConnectionSelector);
        assert_eq!(state.ui.connection_list_selected, 0);
    }

    #[test]
    fn sorts_profiles_before_presenting_selector() {
        let mut state = AppState::new("test".to_string());

        initialize_connection_state(
            &mut state,
            &crate::services::AppServices::stub(),
            Ok(vec![profile("zeta"), profile("alpha")]),
            Some(Err(ServiceFileError::NotFound("/tmp/pg_service.conf".into()))),
        )
        .unwrap();

        assert_eq!(state.input_mode(), InputMode::ConnectionSelector);
        assert_eq!(state.connections()[0].display_name(), "alpha");
        assert_eq!(state.connections()[1].display_name(), "zeta");
    }

    #[test]
    fn reports_version_mismatch() {
        let mut state = AppState::new("test".to_string());

        let error = initialize_connection_state(
            &mut state,
            &crate::services::AppServices::stub(),
            Err(ConnectionStoreError::VersionMismatch {
                found: 1,
                expected: 2,
            }),
            None,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            StartupLoadError::VersionMismatch {
                found: 1,
                expected: 2
            }
        ));
    }
}
