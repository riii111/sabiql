pub(in crate::adapters::mysql) fn build_metadata_select_query(
    query: &str,
    source_alias: &str,
    marker_alias: &str,
) -> String {
    format!(
        "WITH {source_alias} AS (SELECT * FROM (({query}\n) LIMIT 0) AS __sabiql_metadata_inner) SELECT {source_alias}.* FROM {source_alias} RIGHT JOIN (SELECT 1 AS {marker_alias}) AS __sabiql_metadata_marker ON TRUE"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_metadata_select_query_without_changing_the_fallback_sql() {
        assert_eq!(
            build_metadata_select_query("SELECT 1", "__source", "__marker"),
            "WITH __source AS (SELECT * FROM ((SELECT 1\n) LIMIT 0) AS __sabiql_metadata_inner) SELECT __source.* FROM __source RIGHT JOIN (SELECT 1 AS __marker) AS __sabiql_metadata_marker ON TRUE"
        );
    }
}
