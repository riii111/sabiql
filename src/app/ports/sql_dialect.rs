pub trait SqlDialect: Send + Sync {
    fn quote_ident(&self, name: &str) -> String;
    fn quote_literal(&self, value: &str) -> String;
    fn build_update_sql(
        &self,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &str,
        pk_pairs: &[(String, String)],
    ) -> String;
    fn build_bulk_delete_sql(
        &self,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, String)>],
    ) -> String;
}
