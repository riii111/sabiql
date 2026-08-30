use std::sync::Arc;

use super::ports::outbound::{DdlGenerator, DsnBuilder};

pub struct AppServices {
    pub ddl_generator: Arc<dyn DdlGenerator>,
    pub dsn_builder: Arc<dyn DsnBuilder>,
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use std::sync::Arc;

    use super::{AppServices, DdlGenerator, DsnBuilder};
    use crate::domain::{ConnectionProfile, DatabaseType, Table};

    impl AppServices {
        #[doc(hidden)]
        pub fn stub() -> Self {
            struct StubDdlGenerator;
            impl DdlGenerator for StubDdlGenerator {
                fn generate_ddl(&self, _database_type: DatabaseType, _table: &Table) -> String {
                    String::new()
                }
            }

            struct StubDsnBuilder;
            impl DsnBuilder for StubDsnBuilder {
                fn build_dsn(&self, _profile: &ConnectionProfile) -> String {
                    "stub-dsn".to_string()
                }
            }

            Self {
                ddl_generator: Arc::new(StubDdlGenerator),
                dsn_builder: Arc::new(StubDsnBuilder),
            }
        }
    }
}
