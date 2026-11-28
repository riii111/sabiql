use crate::app::ports::outbound::DbOperationError;
use crate::domain::{
    Column, ColumnAttributes, FkAction, ForeignKey, Index, IndexAttributes, IndexType, RlsCommand,
    RlsInfo, RlsPolicy, Schema, TableSignature, TableSummary, Trigger,
};

use super::super::super::PostgresAdapter;

pub(in crate::adapters::postgres) type TableDetailCombined = (
    bool,
    Vec<Column>,
    Vec<Index>,
    Vec<ForeignKey>,
    Option<RlsInfo>,
    Vec<Trigger>,
    TableInfo,
);

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
enum MetadataParseError {
    #[error("foreign key action parse failed: {0}")]
    ForeignKeyAction(String),
    #[error("RLS command parse failed: {0}")]
    RlsCommand(String),
}

impl From<MetadataParseError> for DbOperationError {
    fn from(error: MetadataParseError) -> Self {
        Self::MetadataParseFailed(error.to_string())
    }
}

fn detail_json<'a>(raw: &'a str, operation: &str) -> Result<&'a str, DbOperationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Err(DbOperationError::EmptyResponse(operation.to_string()));
    }
    Ok(trimmed)
}

#[derive(Default, serde::Deserialize)]
pub(in crate::adapters::postgres) struct TableInfo {
    pub owner: Option<String>,
    pub comment: Option<String>,
    pub row_count_estimate: Option<i64>,
}

#[derive(serde::Deserialize)]
struct RawColumn {
    name: String,
    data_type: String,
    nullable: bool,
    default: Option<String>,
    is_primary_key: bool,
    is_unique: bool,
    comment: Option<String>,
    ordinal_position: i32,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "PostgreSQL index flags are independent catalog fields"
)]
#[derive(serde::Deserialize)]
struct RawIndex {
    name: String,
    columns: Vec<String>,
    is_unique: bool,
    is_primary: bool,
    is_partial: bool,
    has_expression: bool,
    index_type: String,
    definition: Option<String>,
}

#[derive(serde::Deserialize)]
struct RawForeignKey {
    name: String,
    from_schema: String,
    from_table: String,
    from_columns: Vec<String>,
    to_schema: String,
    to_table: String,
    to_columns: Vec<String>,
    on_delete: String,
    on_update: String,
}

#[derive(serde::Deserialize)]
struct RawRls {
    enabled: bool,
    force: bool,
    policies: Vec<RawPolicy>,
}

#[derive(serde::Deserialize)]
struct RawPolicy {
    name: String,
    permissive: bool,
    roles: Vec<String>,
    cmd: String,
    qual: Option<String>,
    with_check: Option<String>,
}

#[derive(serde::Deserialize)]
struct RawTrigger {
    name: String,
    definition: String,
}

impl From<RawColumn> for Column {
    fn from(c: RawColumn) -> Self {
        Self {
            name: c.name,
            data_type: c.data_type,
            default: c.default,
            attributes: ColumnAttributes::from_parts(c.nullable, c.is_primary_key, c.is_unique),
            comment: c.comment,
            ordinal_position: c.ordinal_position,
            character_set_name: None,
            collation_name: None,
            generation_expression: None,
            generation_kind: None,
        }
    }
}

impl From<RawIndex> for Index {
    fn from(i: RawIndex) -> Self {
        let mut attributes = IndexAttributes::from_parts(i.is_unique, i.is_primary);
        if i.is_partial {
            attributes = attributes | IndexAttributes::PARTIAL;
        }
        if i.has_expression {
            attributes = attributes | IndexAttributes::EXPRESSION;
        }

        Self {
            name: i.name,
            columns: i.columns,
            attributes,
            index_type: match i.index_type.parse::<IndexType>() {
                Ok(index_type) => index_type,
                Err(never) => match never {},
            },
            definition: i.definition,
        }
    }
}

impl TryFrom<RawForeignKey> for ForeignKey {
    type Error = MetadataParseError;

    fn try_from(fk: RawForeignKey) -> Result<Self, Self::Error> {
        let on_delete = fk
            .on_delete
            .parse::<FkAction>()
            .map_err(|error| MetadataParseError::ForeignKeyAction(error.to_string()))?;
        let on_update = fk
            .on_update
            .parse::<FkAction>()
            .map_err(|error| MetadataParseError::ForeignKeyAction(error.to_string()))?;
        Ok(Self {
            name: fk.name,
            from_schema: fk.from_schema,
            from_table: fk.from_table,
            from_columns: fk.from_columns,
            to_schema: fk.to_schema,
            to_table: fk.to_table,
            to_columns: fk.to_columns,
            on_delete,
            on_update,
            reference_resolved: true,
        })
    }
}

impl TryFrom<RawRls> for RlsInfo {
    type Error = MetadataParseError;

    fn try_from(raw: RawRls) -> Result<Self, Self::Error> {
        let policies = raw
            .policies
            .into_iter()
            .map(|p| {
                let cmd = p
                    .cmd
                    .parse::<RlsCommand>()
                    .map_err(|error| MetadataParseError::RlsCommand(error.to_string()))?;
                Ok(RlsPolicy {
                    name: p.name,
                    permissive: p.permissive,
                    roles: p.roles,
                    cmd,
                    qual: p.qual,
                    with_check: p.with_check,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            enabled: raw.enabled,
            force: raw.force,
            policies,
        })
    }
}

impl PostgresAdapter {
    pub(in crate::adapters::postgres) fn parse_tables(
        json: &str,
    ) -> Result<Vec<TableSummary>, DbOperationError> {
        #[derive(serde::Deserialize)]
        struct RawTable {
            schema: String,
            name: String,
            row_count_estimate: Option<i64>,
        }

        let raw: Vec<RawTable> = serde_json::from_str(json)?;

        Ok(raw
            .into_iter()
            .map(|t| TableSummary::new(t.schema, t.name, t.row_count_estimate))
            .collect())
    }

    pub(in crate::adapters::postgres) fn parse_table_signatures(
        json: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        #[derive(serde::Deserialize)]
        struct RawTableSignature {
            schema: String,
            name: String,
            signature: String,
        }

        let raw: Vec<RawTableSignature> = serde_json::from_str(json)?;

        Ok(raw
            .into_iter()
            .map(|t| TableSignature {
                schema: t.schema,
                name: t.name,
                signature: t.signature,
            })
            .collect())
    }

    pub(in crate::adapters::postgres) fn parse_schemas(
        json: &str,
    ) -> Result<Vec<Schema>, DbOperationError> {
        #[derive(serde::Deserialize)]
        struct RawSchema {
            name: String,
        }

        let raw: Vec<RawSchema> = serde_json::from_str(json)?;

        Ok(raw.into_iter().map(|s| Schema::new(s.name)).collect())
    }

    pub(in crate::adapters::postgres) fn parse_table_detail_combined(
        json: &str,
    ) -> Result<TableDetailCombined, DbOperationError> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct CombinedDetail {
            exists: bool,
            columns: Vec<RawColumn>,
            indexes: Vec<RawIndex>,
            foreign_keys: Vec<RawForeignKey>,
            #[serde(deserialize_with = "serde::Deserialize::deserialize")]
            rls: Option<RawRls>,
            triggers: Vec<RawTrigger>,
            #[serde(deserialize_with = "serde::Deserialize::deserialize")]
            table_info: Option<TableInfo>,
        }

        let detail: CombinedDetail =
            serde_json::from_str(detail_json(json, "table_detail_combined")?)?;
        let mut table_info = detail.table_info.unwrap_or_default();
        // PostgreSQL returns reltuples = -1 when VACUUM/ANALYZE has never run.
        table_info.row_count_estimate = table_info.row_count_estimate.filter(|&n| n >= 0);

        Ok((
            detail.exists,
            detail.columns.into_iter().map(Column::from).collect(),
            detail.indexes.into_iter().map(Index::from).collect(),
            detail
                .foreign_keys
                .into_iter()
                .map(ForeignKey::try_from)
                .collect::<Result<_, _>>()?,
            detail.rls.map(RlsInfo::try_from).transpose()?,
            detail
                .triggers
                .into_iter()
                .map(|trigger| Trigger {
                    name: trigger.name,
                    definition: trigger.definition,
                })
                .collect(),
            table_info,
        ))
    }

    pub(in crate::adapters::postgres) fn parse_table_columns_and_fks(
        json: &str,
    ) -> Result<(bool, Vec<Column>, Vec<ForeignKey>), DbOperationError> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct LightDetail {
            exists: bool,
            columns: Vec<RawColumn>,
            foreign_keys: Vec<RawForeignKey>,
        }

        let detail: LightDetail =
            serde_json::from_str(detail_json(json, "table_columns_and_fks")?)?;

        Ok((
            detail.exists,
            detail.columns.into_iter().map(Column::from).collect(),
            detail
                .foreign_keys
                .into_iter()
                .map(ForeignKey::try_from)
                .collect::<Result<_, _>>()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::postgres::PostgresAdapter;
    use crate::app::ports::outbound::DbOperationError;

    mod table_signature_parsing {
        use super::*;

        #[test]
        fn valid_single_signature_parses_all_fields() {
            let json = r#"[{
                "schema": "public",
                "name": "users",
                "signature": "abc123def456"
            }]"#;

            let result = PostgresAdapter::parse_table_signatures(json).unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].schema, "public");
            assert_eq!(result[0].name, "users");
            assert_eq!(result[0].signature, "abc123def456");
            assert_eq!(result[0].qualified_name(), "public.users");
        }

        #[test]
        fn multiple_signatures_parse_in_order() {
            let json = r#"[
                {"schema": "public", "name": "users", "signature": "aaa"},
                {"schema": "auth", "name": "sessions", "signature": "bbb"}
            ]"#;

            let result = PostgresAdapter::parse_table_signatures(json).unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].qualified_name(), "public.users");
            assert_eq!(result[1].qualified_name(), "auth.sessions");
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_table_signatures("{not valid}");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn missing_field_returns_error() {
            let json = r#"[{"schema": "public", "name": "users"}]"#;
            let result = PostgresAdapter::parse_table_signatures(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }
    }

    mod schema_parsing {
        use super::*;

        #[test]
        fn valid_multiple_schemas_parse_in_order() {
            let json = r#"[{"name": "public"}, {"name": "auth"}, {"name": "custom"}]"#;
            let result = PostgresAdapter::parse_schemas(json).unwrap();

            assert_eq!(result.len(), 3);
            assert_eq!(result[0].name, "public");
            assert_eq!(result[1].name, "auth");
            assert_eq!(result[2].name, "custom");
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_schemas("{not valid json}");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn missing_name_field_returns_error() {
            let json = r"[{}]";
            let result = PostgresAdapter::parse_schemas(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn wrong_structure_returns_error() {
            let json = r#"["public", "auth"]"#;
            let result = PostgresAdapter::parse_schemas(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn empty_array_returns_empty_schema_vec() {
            let json = r"[]";
            let result = PostgresAdapter::parse_schemas(json).unwrap();
            assert!(result.is_empty());
        }
    }

    mod table_detail_combined_parsing {
        use super::*;
        use crate::domain::{IndexType, RlsCommand};

        fn build_combined_json(
            exists: bool,
            columns: &str,
            indexes: &str,
            fks: &str,
            rls: &str,
            triggers: &str,
            table_info: &str,
        ) -> String {
            format!(
                r#"{{
                    "exists": {exists},
                    "columns": {columns},
                    "indexes": {indexes},
                    "foreign_keys": {fks},
                    "rls": {rls},
                    "triggers": {triggers},
                    "table_info": {table_info}
                }}"#
            )
        }

        #[test]
        fn valid_combined_json_parses_all_categories() {
            let json = build_combined_json(
                true,
                r#"[{"name":"id","data_type":"integer","nullable":false,"default":null,"is_primary_key":true,"is_unique":false,"comment":null,"ordinal_position":1}]"#,
                r#"[{"name":"users_idx","columns":["id","lower(email)"],"is_unique":true,"is_primary":false,"is_partial":true,"has_expression":true,"index_type":"ivfflat"}]"#,
                r#"[{"name":"fk_1","from_schema":"public","from_table":"users","from_columns":["org_id"],"to_schema":"public","to_table":"orgs","to_columns":["id"],"on_delete":"c","on_update":"a"}]"#,
                r#"{"enabled":true,"force":false,"policies":[{"name":"read_users","permissive":true,"roles":[],"cmd":"r","qual":"true"}]}"#,
                r#"[{"name":"audit","definition":"CREATE TRIGGER audit AFTER INSERT ON users FOR EACH ROW EXECUTE FUNCTION audit()"}]"#,
                r#"{"owner":"postgres","comment":null,"row_count_estimate":-1}"#,
            );

            let (exists, columns, indexes, fks, rls, triggers, table_info) =
                PostgresAdapter::parse_table_detail_combined(&json).unwrap();

            assert!(exists);
            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "id");
            assert!(columns[0].is_primary_key());
            assert_eq!(indexes[0].columns, ["id", "lower(email)"]);
            assert!(indexes[0].is_unique());
            assert!(indexes[0].is_partial());
            assert!(indexes[0].has_expression());
            assert_eq!(
                indexes[0].index_type,
                IndexType::Other("ivfflat".to_string())
            );
            assert_eq!(fks[0].referenced_table(), "public.orgs");
            let rls = rls.unwrap();
            assert!(rls.enabled);
            assert_eq!(rls.policies[0].cmd, RlsCommand::Select);
            assert!(rls.policies[0].roles.is_empty());
            assert!(triggers[0].definition.starts_with("CREATE TRIGGER audit"));
            assert_eq!(table_info.owner.as_deref(), Some("postgres"));
            assert_eq!(table_info.row_count_estimate, None);
        }

        #[test]
        fn existing_zero_column_table_keeps_empty_metadata_distinct_from_missing() {
            let json = build_combined_json(
                true,
                "[]",
                "[]",
                "[]",
                r#"{"enabled":false,"force":false,"policies":[]}"#,
                "[]",
                r#"{"owner":"postgres","row_count_estimate":0}"#,
            );

            let (exists, columns, indexes, fks, rls, triggers, table_info) =
                PostgresAdapter::parse_table_detail_combined(&json).unwrap();

            assert!(exists);
            assert!(columns.is_empty());
            assert!(indexes.is_empty());
            assert!(fks.is_empty());
            assert!(rls.unwrap().policies.is_empty());
            assert!(triggers.is_empty());
            assert_eq!(table_info.row_count_estimate, Some(0));
        }

        #[test]
        fn missing_relation_preserves_false_existence_bit() {
            let json = build_combined_json(false, "[]", "[]", "[]", "null", "[]", "null");

            let (exists, columns, indexes, fks, rls, triggers, table_info) =
                PostgresAdapter::parse_table_detail_combined(&json).unwrap();

            assert!(!exists);
            assert!(columns.is_empty());
            assert!(indexes.is_empty());
            assert!(fks.is_empty());
            assert!(rls.is_none());
            assert!(triggers.is_empty());
            assert!(table_info.owner.is_none());
        }

        #[test]
        fn missing_key_returns_invalid_json_error() {
            let json = r#"{"exists": false, "columns": [], "indexes": [], "foreign_keys": [], "triggers": [], "table_info": null}"#;
            let result = PostgresAdapter::parse_table_detail_combined(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn unknown_key_returns_invalid_json_error() {
            let json = build_combined_json(true, "[]", "[]", "[]", "null", "[]", "null")
                .replace('}', r#","extra_key": null}"#);
            let result = PostgresAdapter::parse_table_detail_combined(&json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn empty_input_returns_error_for_combined_detail() {
            let result = PostgresAdapter::parse_table_detail_combined("");
            assert!(matches!(result, Err(DbOperationError::EmptyResponse(_))));
        }

        #[test]
        fn null_input_returns_error_for_combined_detail() {
            let result = PostgresAdapter::parse_table_detail_combined("null");
            assert!(matches!(result, Err(DbOperationError::EmptyResponse(_))));
        }
    }

    mod table_columns_and_fks_parsing {
        use super::*;
        use crate::domain::FkAction;

        fn build_light_json(exists: bool, columns: &str, fks: &str) -> String {
            format!(r#"{{"exists": {exists}, "columns": {columns}, "foreign_keys": {fks}}}"#)
        }

        #[test]
        fn valid_light_json_parses_columns_and_fks() {
            let json = build_light_json(
                true,
                r#"[{"name":"id","data_type":"integer","nullable":false,"default":null,"is_primary_key":true,"is_unique":false,"comment":null,"ordinal_position":1}]"#,
                r#"[{"name":"fk_1","from_schema":"public","from_table":"orders","from_columns":["user_id"],"to_schema":"public","to_table":"users","to_columns":["id"],"on_delete":"c","on_update":"a"}]"#,
            );

            let (exists, columns, fks) =
                PostgresAdapter::parse_table_columns_and_fks(&json).unwrap();

            assert!(exists);
            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "id");
            assert_eq!(fks.len(), 1);
            assert_eq!(fks[0].name, "fk_1");
            assert_eq!(fks[0].on_delete, FkAction::Cascade);
            assert_eq!(fks[0].on_update, FkAction::NoAction);
        }

        #[test]
        fn empty_arrays_parse_to_empty() {
            let json = build_light_json(true, "[]", "[]");

            let (exists, columns, fks) =
                PostgresAdapter::parse_table_columns_and_fks(&json).unwrap();

            assert!(exists);
            assert!(columns.is_empty());
            assert!(fks.is_empty());
        }

        #[test]
        fn missing_relation_preserves_false_existence_bit() {
            let json = build_light_json(false, "[]", "[]");

            let (exists, columns, fks) =
                PostgresAdapter::parse_table_columns_and_fks(&json).unwrap();

            assert!(!exists);
            assert!(columns.is_empty());
            assert!(fks.is_empty());
        }

        #[test]
        fn missing_key_returns_error() {
            let json = r#"{"exists": true, "columns": []}"#;
            let result = PostgresAdapter::parse_table_columns_and_fks(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn unknown_key_returns_error() {
            let json = r#"{"exists": true, "columns": [], "foreign_keys": [], "extra": null}"#;
            let result = PostgresAdapter::parse_table_columns_and_fks(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn empty_input_returns_error_for_columns_and_fks() {
            let result = PostgresAdapter::parse_table_columns_and_fks("");
            assert!(matches!(result, Err(DbOperationError::EmptyResponse(_))));
        }

        #[test]
        fn null_input_returns_error_for_columns_and_fks() {
            let result = PostgresAdapter::parse_table_columns_and_fks("null");
            assert!(matches!(result, Err(DbOperationError::EmptyResponse(_))));
        }
    }
}
