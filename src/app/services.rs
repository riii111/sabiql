use std::sync::Arc;

use super::ports::{DdlGenerator, SqlDialect};
use crate::app::model::shared::db_capabilities::DbCapabilities;

pub struct AppServices {
    pub ddl_generator: Arc<dyn DdlGenerator>,
    pub sql_dialect: Arc<dyn SqlDialect>,
    pub db_capabilities: DbCapabilities,
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
            fn build_explain_sql(&self, _query: &str) -> String {
                format!("EXPLAIN {_query}")
            }

            fn build_explain_analyze_sql(&self, _query: &str) -> String {
                format!("EXPLAIN ANALYZE {_query}")
            }

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
            db_capabilities: DbCapabilities::new(
                true,
                true,
                vec![
                    crate::app::model::shared::inspector_tab::InspectorTab::Info,
                    crate::app::model::shared::inspector_tab::InspectorTab::Columns,
                    crate::app::model::shared::inspector_tab::InspectorTab::Indexes,
                    crate::app::model::shared::inspector_tab::InspectorTab::ForeignKeys,
                    crate::app::model::shared::inspector_tab::InspectorTab::Rls,
                    crate::app::model::shared::inspector_tab::InspectorTab::Triggers,
                    crate::app::model::shared::inspector_tab::InspectorTab::Ddl,
                ],
            ),
        }
    }
}
