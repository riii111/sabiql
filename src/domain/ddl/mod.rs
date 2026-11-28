mod postgres;
mod sqlite;

use crate::{DatabaseType, Table};

pub fn generate_ddl(database_type: DatabaseType, table: &Table) -> String {
    match database_type {
        DatabaseType::PostgreSQL => postgres::generate_ddl(table),
        DatabaseType::SQLite => sqlite::generate_ddl(table),
        DatabaseType::MySQL => table.source_ddl().unwrap_or_default().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::generate_ddl;
    use crate::DatabaseType;
    use crate::test_support::table;

    #[test]
    fn mysql_ddl_is_the_source_ddl() {
        let source_ddl = "CREATE TABLE `users` (`id` INT)";
        let mut table = table::minimal("app", "users");
        table.source_ddl = Some(source_ddl.to_string());

        assert_eq!(generate_ddl(DatabaseType::MySQL, &table), source_ddl);
    }
}
