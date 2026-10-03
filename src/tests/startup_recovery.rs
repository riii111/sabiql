use sabiql_app::domain::connection::{ConnectionId, ConnectionProfile};
use sabiql_app::model::app_state::AppState;
use sabiql_app::model::shared::input_mode::InputMode;
use sabiql_app::ports::outbound::connection_store::SecretStoreFailure;
use sabiql_app::ports::outbound::{
    ConnectionStore, ConnectionStoreError, PgServiceEntryReader, ServiceFileContents,
    ServiceFileError,
};

use crate::initialize_connection_list;

struct Store(Result<Vec<ConnectionProfile>, ConnectionStoreError>);
impl ConnectionStore for Store {
    fn load_all(&self) -> Result<Vec<ConnectionProfile>, ConnectionStoreError> {
        self.0.clone()
    }
    fn save(&self, _: &ConnectionProfile) -> Result<(), ConnectionStoreError> {
        panic!("startup must not save")
    }
    fn delete(&self, _: &ConnectionId) -> Result<(), ConnectionStoreError> {
        panic!("startup must not delete")
    }
    fn find_by_id(
        &self,
        _: &ConnectionId,
    ) -> Result<Option<ConnectionProfile>, ConnectionStoreError> {
        panic!("startup only loads")
    }
}
struct NoServices;
impl PgServiceEntryReader for NoServices {
    fn read_services(&self) -> Result<ServiceFileContents, ServiceFileError> {
        Ok(ServiceFileContents {
            entries: vec![],
            warning: None,
        })
    }
}

#[test]
fn failed_saved_credentials_stop_startup_without_first_run_or_secret_echo() {
    for error in [
        ConnectionStoreError::SecretStore(SecretStoreFailure::Unavailable),
        ConnectionStoreError::SecretStore(SecretStoreFailure::TimedOut),
        ConnectionStoreError::InvalidPasswordReference("synthetic-secret".into()),
        ConnectionStoreError::DuplicatePasswordReference("synthetic-secret".into()),
        ConnectionStoreError::VersionMismatch {
            found: 99,
            expected: 4,
        },
    ] {
        let mut state = AppState::new("fixture".into());
        let result = initialize_connection_list(&mut state, false, &Store(Err(error)), &NoServices);
        let message = result.unwrap_err().to_string();
        assert!(message.contains("preserved"));
        assert!(!message.contains("synthetic-secret"));
        assert!(!message.contains("Please delete"));
        assert_ne!(state.modal.active_mode(), InputMode::ConnectionSetup);
    }
}

#[test]
fn cli_target_survives_saved_connection_failure_without_first_run() {
    let mut state = AppState::new("fixture".into());
    let store = Store(Err(ConnectionStoreError::SecretStore(
        SecretStoreFailure::Unavailable,
    )));
    initialize_connection_list(&mut state, true, &store, &NoServices).unwrap();
    assert_ne!(state.modal.active_mode(), InputMode::ConnectionSetup);
}

#[test]
fn genuinely_empty_configuration_still_opens_initial_setup() {
    let mut state = AppState::new("fixture".into());
    initialize_connection_list(&mut state, false, &Store(Ok(vec![])), &NoServices).unwrap();
    assert_eq!(state.modal.active_mode(), InputMode::ConnectionSetup);
}
