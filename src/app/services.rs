use std::sync::Arc;

use super::ports::{DdlGenerator, SqlDialect};

pub struct AppServices {
    pub ddl_generator: Arc<dyn DdlGenerator>,
    pub sql_dialect: Arc<dyn SqlDialect>,
}

#[cfg(test)]
impl AppServices {
    pub fn stub() -> Self {
        struct StubDdlGenerator;
        impl DdlGenerator for StubDdlGenerator {
            fn generate_ddl(&self, _table: &crate::domain::Table) -> String {
                unimplemented!("inject a real DdlGenerator via AppServices")
            }
            fn ddl_line_count(&self, _table: &crate::domain::Table) -> usize {
                0
            }
        }

        struct StubSqlDialect;
        impl SqlDialect for StubSqlDialect {
            fn build_update_sql(
                &self,
                _schema: &str,
                _table: &str,
                _column: &str,
                _new_value: &str,
                _pk_pairs: &[(String, String)],
            ) -> String {
                unimplemented!("inject a real SqlDialect via AppServices")
            }
            fn build_bulk_delete_sql(
                &self,
                _schema: &str,
                _table: &str,
                _pk_pairs_per_row: &[Vec<(String, String)>],
            ) -> String {
                unimplemented!("inject a real SqlDialect via AppServices")
            }
        }

        Self {
            ddl_generator: Arc::new(StubDdlGenerator),
            sql_dialect: Arc::new(StubSqlDialect),
        }
    }
}
