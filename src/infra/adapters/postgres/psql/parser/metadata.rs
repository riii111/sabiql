use crate::app::ports::DbOperationError;
use crate::domain::{
    Column, FkAction, ForeignKey, Index, IndexType, RlsCommand, RlsInfo, RlsPolicy, Schema,
    TableSignature, TableSummary, Trigger, TriggerEvent, TriggerTiming,
};

use super::super::super::PostgresAdapter;

pub(in crate::infra::adapters::postgres) type TableDetailCombined = (
    Vec<Column>,
    Vec<Index>,
    Vec<ForeignKey>,
    Option<RlsInfo>,
    Vec<Trigger>,
    TableInfo,
);

fn non_empty_json(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        None
    } else {
        Some(trimmed)
    }
}

pub(in crate::infra::adapters::postgres) struct TableInfo {
    pub owner: Option<String>,
    pub comment: Option<String>,
    pub row_count_estimate: Option<i64>,
}

impl PostgresAdapter {
    pub(in crate::infra::adapters::postgres) fn parse_table_info(
        json: &str,
    ) -> Result<TableInfo, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(TableInfo {
                owner: None,
                comment: None,
                row_count_estimate: None,
            });
        };

        #[derive(serde::Deserialize)]
        struct RawTableInfo {
            owner: Option<String>,
            comment: Option<String>,
            row_count_estimate: Option<i64>,
        }

        let raw: RawTableInfo = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        // PostgreSQL returns reltuples = -1 when VACUUM/ANALYZE has never run
        let row_count = raw.row_count_estimate.filter(|&n| n >= 0);

        Ok(TableInfo {
            owner: raw.owner,
            comment: raw.comment,
            row_count_estimate: row_count,
        })
    }

    pub(in crate::infra::adapters::postgres) fn parse_tables(
        json: &str,
    ) -> Result<Vec<TableSummary>, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(Vec::new());
        };

        #[derive(serde::Deserialize)]
        struct RawTable {
            schema: String,
            name: String,
            row_count_estimate: Option<i64>,
            has_rls: bool,
        }

        let raw: Vec<RawTable> = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|t| TableSummary::new(t.schema, t.name, t.row_count_estimate, t.has_rls))
            .collect())
    }

    pub(in crate::infra::adapters::postgres) fn parse_table_signatures(
        json: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(Vec::new());
        };

        #[derive(serde::Deserialize)]
        struct RawTableSignature {
            schema: String,
            name: String,
            signature: String,
        }

        let raw: Vec<RawTableSignature> = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|t| TableSignature {
                schema: t.schema,
                name: t.name,
                signature: t.signature,
            })
            .collect())
    }

    pub(in crate::infra::adapters::postgres) fn parse_schemas(
        json: &str,
    ) -> Result<Vec<Schema>, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(Vec::new());
        };

        #[derive(serde::Deserialize)]
        struct RawSchema {
            name: String,
        }

        let raw: Vec<RawSchema> = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        Ok(raw.into_iter().map(|s| Schema::new(s.name)).collect())
    }

    pub(in crate::infra::adapters::postgres) fn parse_columns(
        json: &str,
    ) -> Result<Vec<Column>, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(Vec::new());
        };

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

        let raw: Vec<RawColumn> = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|c| Column {
                name: c.name,
                data_type: c.data_type,
                nullable: c.nullable,
                default: c.default,
                is_primary_key: c.is_primary_key,
                is_unique: c.is_unique,
                comment: c.comment,
                ordinal_position: c.ordinal_position,
            })
            .collect())
    }

    pub(in crate::infra::adapters::postgres) fn parse_indexes(
        json: &str,
    ) -> Result<Vec<Index>, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(Vec::new());
        };

        #[derive(serde::Deserialize)]
        struct RawIndex {
            name: String,
            columns: Vec<String>,
            is_unique: bool,
            is_primary: bool,
            index_type: String,
            definition: Option<String>,
        }

        let raw: Vec<RawIndex> = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|i| Index {
                name: i.name,
                columns: i.columns,
                is_unique: i.is_unique,
                is_primary: i.is_primary,
                index_type: match i.index_type.as_str() {
                    "btree" => IndexType::BTree,
                    "hash" => IndexType::Hash,
                    "gist" => IndexType::Gist,
                    "gin" => IndexType::Gin,
                    "brin" => IndexType::Brin,
                    other => IndexType::Other(other.to_string()),
                },
                definition: i.definition,
            })
            .collect())
    }

    pub(in crate::infra::adapters::postgres) fn parse_foreign_keys(
        json: &str,
    ) -> Result<Vec<ForeignKey>, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(Vec::new());
        };

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

        fn parse_fk_action(code: &str) -> FkAction {
            match code {
                "r" => FkAction::Restrict,
                "c" => FkAction::Cascade,
                "n" => FkAction::SetNull,
                "d" => FkAction::SetDefault,
                _ => FkAction::NoAction,
            }
        }

        let raw: Vec<RawForeignKey> = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|fk| ForeignKey {
                name: fk.name,
                from_schema: fk.from_schema,
                from_table: fk.from_table,
                from_columns: fk.from_columns,
                to_schema: fk.to_schema,
                to_table: fk.to_table,
                to_columns: fk.to_columns,
                on_delete: parse_fk_action(&fk.on_delete),
                on_update: parse_fk_action(&fk.on_update),
            })
            .collect())
    }

    pub(in crate::infra::adapters::postgres) fn parse_rls(
        json: &str,
    ) -> Result<Option<RlsInfo>, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(None);
        };

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
            roles: Option<Vec<String>>,
            cmd: String,
            qual: Option<String>,
            with_check: Option<String>,
        }

        let raw: RawRls = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        let policies = raw
            .policies
            .into_iter()
            .map(|p| RlsPolicy {
                name: p.name,
                permissive: p.permissive,
                roles: p.roles.unwrap_or_default(),
                cmd: match p.cmd.as_str() {
                    "r" => RlsCommand::Select,
                    "a" => RlsCommand::Insert,
                    "w" => RlsCommand::Update,
                    "d" => RlsCommand::Delete,
                    _ => RlsCommand::All,
                },
                qual: p.qual,
                with_check: p.with_check,
            })
            .collect();

        Ok(Some(RlsInfo {
            enabled: raw.enabled,
            force: raw.force,
            policies,
        }))
    }

    pub(in crate::infra::adapters::postgres) fn parse_triggers(
        json: &str,
    ) -> Result<Vec<Trigger>, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(Vec::new());
        };

        #[derive(serde::Deserialize)]
        struct RawTrigger {
            name: String,
            timing: String,
            events: Vec<String>,
            function_name: String,
            security_definer: bool,
        }

        let raw: Vec<RawTrigger> = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|t| Trigger {
                name: t.name,
                timing: match t.timing.as_str() {
                    "BEFORE" => TriggerTiming::Before,
                    "INSTEAD OF" => TriggerTiming::InsteadOf,
                    _ => TriggerTiming::After,
                },
                events: t
                    .events
                    .iter()
                    .filter_map(|e| match e.as_str() {
                        "INSERT" => Some(TriggerEvent::Insert),
                        "UPDATE" => Some(TriggerEvent::Update),
                        "DELETE" => Some(TriggerEvent::Delete),
                        "TRUNCATE" => Some(TriggerEvent::Truncate),
                        _ => None,
                    })
                    .collect(),
                function_name: t.function_name,
                security_definer: t.security_definer,
            })
            .collect())
    }

    pub(in crate::infra::adapters::postgres) fn parse_table_detail_combined(
        json: &str,
    ) -> Result<TableDetailCombined, DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Err(DbOperationError::InvalidJson(
                "table_detail_combined: empty response".to_string(),
            ));
        };

        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct CombinedDetail {
            columns: serde_json::Value,
            indexes: serde_json::Value,
            foreign_keys: serde_json::Value,
            rls: serde_json::Value,
            triggers: serde_json::Value,
            table_info: serde_json::Value,
        }

        let combined: CombinedDetail = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        let columns = Self::parse_columns(&combined.columns.to_string())?;
        let indexes = Self::parse_indexes(&combined.indexes.to_string())?;
        let foreign_keys = Self::parse_foreign_keys(&combined.foreign_keys.to_string())?;
        let rls = Self::parse_rls(&combined.rls.to_string())?;
        let triggers = Self::parse_triggers(&combined.triggers.to_string())?;
        let table_info = Self::parse_table_info(&combined.table_info.to_string())?;

        Ok((columns, indexes, foreign_keys, rls, triggers, table_info))
    }

    pub(in crate::infra::adapters::postgres) fn parse_table_columns_and_fks(
        json: &str,
    ) -> Result<(Vec<Column>, Vec<ForeignKey>), DbOperationError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Err(DbOperationError::InvalidJson(
                "table_columns_and_fks: empty response".to_string(),
            ));
        };

        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct LightDetail {
            columns: serde_json::Value,
            foreign_keys: serde_json::Value,
        }

        let light: LightDetail = serde_json::from_str(trimmed)
            .map_err(|e| DbOperationError::InvalidJson(e.to_string()))?;

        let columns = Self::parse_columns(&light.columns.to_string())?;
        let foreign_keys = Self::parse_foreign_keys(&light.foreign_keys.to_string())?;

        Ok((columns, foreign_keys))
    }
}

#[cfg(test)]
mod tests {
    use crate::app::ports::DbOperationError;
    use crate::infra::adapters::postgres::PostgresAdapter;

    mod table_signature_parsing {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_empty_vec(#[case] input: &str) {
            let result = PostgresAdapter::parse_table_signatures(input).unwrap();
            assert!(result.is_empty());
        }

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

    mod rls_parsing {
        use super::*;
        use crate::domain::RlsCommand;
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_none(#[case] input: &str) {
            let result = PostgresAdapter::parse_rls(input).unwrap();
            assert!(result.is_none());
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_rls("{not valid json}");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn disabled_rls_with_no_policies_returns_expected() {
            let json = r#"{"enabled": false, "force": false, "policies": []}"#;

            let result = PostgresAdapter::parse_rls(json).unwrap();
            let rls = result.expect("Should return Some(RlsInfo)");

            assert!(!rls.enabled);
            assert!(!rls.force);
            assert!(rls.policies.is_empty());
        }

        #[test]
        fn enabled_and_forced_rls_returns_expected() {
            let json = r#"{"enabled": true, "force": true, "policies": []}"#;

            let result = PostgresAdapter::parse_rls(json).unwrap();
            let rls = result.unwrap();

            assert!(rls.enabled);
            assert!(rls.force);
        }

        #[test]
        fn single_policy_parses_all_fields() {
            let json = r#"{
                "enabled": true,
                "force": false,
                "policies": [{
                    "name": "tenant_isolation",
                    "permissive": true,
                    "roles": ["app_user", "admin"],
                    "cmd": "r",
                    "qual": "tenant_id = current_setting('app.tenant_id')::int",
                    "with_check": null
                }]
            }"#;

            let result = PostgresAdapter::parse_rls(json).unwrap();
            let rls = result.unwrap();
            let policy = &rls.policies[0];

            assert_eq!(policy.name, "tenant_isolation");
            assert!(policy.permissive);
            assert_eq!(policy.roles, vec!["app_user", "admin"]);
            assert_eq!(policy.cmd, RlsCommand::Select);
            assert!(policy.qual.is_some());
            assert!(policy.with_check.is_none());
        }

        #[rstest]
        #[case("*", RlsCommand::All)]
        #[case("r", RlsCommand::Select)]
        #[case("a", RlsCommand::Insert)]
        #[case("w", RlsCommand::Update)]
        #[case("d", RlsCommand::Delete)]
        #[case("x", RlsCommand::All)] // unknown defaults to All
        fn cmd_mapping_returns_expected(#[case] cmd: &str, #[case] expected: RlsCommand) {
            let json = format!(
                r#"{{"enabled": true, "force": false, "policies": [{{
                    "name": "test", "permissive": true, "roles": null,
                    "cmd": "{cmd}", "qual": null, "with_check": null
                }}]}}"#
            );

            let result = PostgresAdapter::parse_rls(&json).unwrap();
            let rls = result.unwrap();

            assert_eq!(rls.policies[0].cmd, expected);
        }

        #[test]
        fn null_roles_becomes_empty_vec() {
            let json = r#"{
                "enabled": true, "force": false,
                "policies": [{"name": "p", "permissive": true, "roles": null, "cmd": "*", "qual": null, "with_check": null}]
            }"#;

            let result = PostgresAdapter::parse_rls(json).unwrap();
            let rls = result.unwrap();

            assert!(rls.policies[0].roles.is_empty());
        }

        #[test]
        fn missing_required_field_returns_invalid_json_error() {
            let json = r#"{"force": false, "policies": []}"#; // missing 'enabled'

            let result = PostgresAdapter::parse_rls(json);

            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }
    }

    mod json_parse_errors {
        use super::*;

        #[test]
        fn parse_tables_with_malformed_json_returns_error() {
            let result = PostgresAdapter::parse_tables("{not valid json}");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn parse_tables_with_wrong_structure_returns_error() {
            let result = PostgresAdapter::parse_tables(r#"["table1", "table2"]"#);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn parse_columns_with_missing_field_returns_error() {
            let json = r#"[{"name": "id", "nullable": true}]"#;
            let result = PostgresAdapter::parse_columns(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn parse_indexes_with_wrong_type_returns_error() {
            let json =
                r#"[{"name": "idx_test", "columns": "id", "unique": false, "primary": false}]"#;
            let result = PostgresAdapter::parse_indexes(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn parse_empty_string_returns_empty_vec() {
            assert!(PostgresAdapter::parse_tables("").unwrap().is_empty());
            assert!(PostgresAdapter::parse_columns("").unwrap().is_empty());
            assert!(PostgresAdapter::parse_indexes("").unwrap().is_empty());
        }

        #[test]
        fn parse_null_string_returns_empty_vec() {
            assert!(PostgresAdapter::parse_tables("null").unwrap().is_empty());
            assert!(PostgresAdapter::parse_columns("null").unwrap().is_empty());
            assert!(PostgresAdapter::parse_indexes("null").unwrap().is_empty());
        }
    }

    mod table_info_parsing {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_none(#[case] input: &str) {
            let info = PostgresAdapter::parse_table_info(input).unwrap();
            assert!(info.owner.is_none());
            assert!(info.comment.is_none());
            assert!(info.row_count_estimate.is_none());
        }

        #[test]
        fn all_fields_present_returns_values() {
            let json = r#"{"owner": "postgres", "comment": "User accounts table", "row_count_estimate": 100}"#;

            let info = PostgresAdapter::parse_table_info(json).unwrap();

            assert_eq!(info.owner.as_deref(), Some("postgres"));
            assert_eq!(info.comment.as_deref(), Some("User accounts table"));
            assert_eq!(info.row_count_estimate, Some(100));
        }

        #[test]
        fn null_fields_returns_none() {
            let json = r#"{"owner": null, "comment": null, "row_count_estimate": null}"#;

            let info = PostgresAdapter::parse_table_info(json).unwrap();

            assert!(info.owner.is_none());
            assert!(info.comment.is_none());
            assert!(info.row_count_estimate.is_none());
        }

        #[test]
        fn negative_row_count_returns_none() {
            let json = r#"{"owner": "postgres", "comment": null, "row_count_estimate": -1}"#;

            let info = PostgresAdapter::parse_table_info(json).unwrap();

            assert!(info.row_count_estimate.is_none());
        }

        #[test]
        fn zero_row_count_returns_zero() {
            let json = r#"{"owner": "postgres", "comment": null, "row_count_estimate": 0}"#;

            let info = PostgresAdapter::parse_table_info(json).unwrap();

            assert_eq!(info.row_count_estimate, Some(0));
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_table_info("{not valid json}");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }
    }

    mod trigger_parsing {
        use super::*;
        use crate::domain::{TriggerEvent, TriggerTiming};
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_empty_vec(#[case] input: &str) {
            let result = PostgresAdapter::parse_triggers(input).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn valid_single_trigger_parses_all_fields() {
            let json = r#"[{
                "name": "audit_trigger",
                "timing": "AFTER",
                "events": ["INSERT", "UPDATE"],
                "function_name": "audit_func",
                "security_definer": true
            }]"#;

            let result = PostgresAdapter::parse_triggers(json).unwrap();
            let trigger = &result[0];

            assert_eq!(result.len(), 1);
            assert_eq!(trigger.name, "audit_trigger");
            assert_eq!(trigger.timing, TriggerTiming::After);
            assert_eq!(
                trigger.events,
                vec![TriggerEvent::Insert, TriggerEvent::Update]
            );
            assert_eq!(trigger.function_name, "audit_func");
            assert!(trigger.security_definer);
        }

        #[rstest]
        #[case("BEFORE", TriggerTiming::Before)]
        #[case("AFTER", TriggerTiming::After)]
        #[case("INSTEAD OF", TriggerTiming::InsteadOf)]
        #[case("UNKNOWN", TriggerTiming::After)] // unknown defaults to After
        fn timing_mapping_returns_expected(#[case] timing: &str, #[case] expected: TriggerTiming) {
            let json = format!(
                r#"[{{
                    "name": "test", "timing": "{timing}", "events": ["INSERT"],
                    "function_name": "func", "security_definer": false
                }}]"#
            );

            let result = PostgresAdapter::parse_triggers(&json).unwrap();
            assert_eq!(result[0].timing, expected);
        }

        #[test]
        fn multiple_events_parsed_in_order() {
            let json = r#"[{
                "name": "multi_event",
                "timing": "BEFORE",
                "events": ["INSERT", "DELETE", "UPDATE", "TRUNCATE"],
                "function_name": "func",
                "security_definer": false
            }]"#;

            let result = PostgresAdapter::parse_triggers(json).unwrap();
            assert_eq!(
                result[0].events,
                vec![
                    TriggerEvent::Insert,
                    TriggerEvent::Delete,
                    TriggerEvent::Update,
                    TriggerEvent::Truncate,
                ]
            );
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_triggers("{not valid json}");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn empty_array_returns_empty_vec() {
            let json = r"[]";
            let result = PostgresAdapter::parse_triggers(json).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn security_definer_false_returns_expected() {
            let json = r#"[{
                "name": "test",
                "timing": "AFTER",
                "events": ["INSERT"],
                "function_name": "func",
                "security_definer": false
            }]"#;

            let result = PostgresAdapter::parse_triggers(json).unwrap();
            assert!(!result[0].security_definer);
        }
    }

    mod schema_parsing {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_empty_vec(#[case] input: &str) {
            let result = PostgresAdapter::parse_schemas(input).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn valid_single_schema_parses_correctly() {
            let json = r#"[{"name": "public"}]"#;
            let result = PostgresAdapter::parse_schemas(json).unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].name, "public");
        }

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
        fn empty_array_returns_empty_vec() {
            let json = r"[]";
            let result = PostgresAdapter::parse_schemas(json).unwrap();
            assert!(result.is_empty());
        }
    }

    mod foreign_key_parsing {
        use super::*;
        use crate::domain::FkAction;
        use rstest::rstest;

        #[rstest]
        #[case("")]
        #[case("null")]
        #[case("   ")]
        fn empty_or_null_input_returns_empty_vec(#[case] input: &str) {
            let result = PostgresAdapter::parse_foreign_keys(input).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn valid_single_fk_parses_all_fields() {
            let json = r#"[{
                "name": "orders_user_fk",
                "from_schema": "public",
                "from_table": "orders",
                "from_columns": ["user_id"],
                "to_schema": "public",
                "to_table": "users",
                "to_columns": ["id"],
                "on_delete": "c",
                "on_update": "a"
            }]"#;

            let result = PostgresAdapter::parse_foreign_keys(json).unwrap();
            let fk = &result[0];

            assert_eq!(result.len(), 1);
            assert_eq!(fk.name, "orders_user_fk");
            assert_eq!(fk.from_schema, "public");
            assert_eq!(fk.from_table, "orders");
            assert_eq!(fk.from_columns, vec!["user_id"]);
            assert_eq!(fk.to_schema, "public");
            assert_eq!(fk.to_table, "users");
            assert_eq!(fk.to_columns, vec!["id"]);
            assert_eq!(fk.on_delete, FkAction::Cascade);
            assert_eq!(fk.on_update, FkAction::NoAction);
        }

        #[rstest]
        #[case("a", FkAction::NoAction)]
        #[case("r", FkAction::Restrict)]
        #[case("c", FkAction::Cascade)]
        #[case("n", FkAction::SetNull)]
        #[case("d", FkAction::SetDefault)]
        #[case("x", FkAction::NoAction)]
        fn fk_action_mapping_returns_expected(
            #[case] action_code: &str,
            #[case] expected: FkAction,
        ) {
            let json = format!(
                r#"[{{
                    "name": "test_fk",
                    "from_schema": "public",
                    "from_table": "t1",
                    "from_columns": ["id"],
                    "to_schema": "public",
                    "to_table": "t2",
                    "to_columns": ["id"],
                    "on_delete": "{action_code}",
                    "on_update": "a"
                }}]"#
            );

            let result = PostgresAdapter::parse_foreign_keys(&json).unwrap();
            assert_eq!(result[0].on_delete, expected);
        }

        #[test]
        fn composite_foreign_key_parses_multiple_columns() {
            let json = r#"[{
                "name": "order_item_fk",
                "from_schema": "public",
                "from_table": "order_items",
                "from_columns": ["order_id", "item_id"],
                "to_schema": "public",
                "to_table": "order_item_master",
                "to_columns": ["order_id", "id"],
                "on_delete": "r",
                "on_update": "r"
            }]"#;

            let result = PostgresAdapter::parse_foreign_keys(json).unwrap();
            let fk = &result[0];

            assert_eq!(fk.from_columns, vec!["order_id", "item_id"]);
            assert_eq!(fk.to_columns, vec!["order_id", "id"]);
            assert_eq!(fk.on_delete, FkAction::Restrict);
            assert_eq!(fk.on_update, FkAction::Restrict);
        }

        #[test]
        fn multiple_foreign_keys_parse_in_order() {
            let json = r#"[
                {
                    "name": "fk_1",
                    "from_schema": "public",
                    "from_table": "t1",
                    "from_columns": ["id"],
                    "to_schema": "public",
                    "to_table": "t2",
                    "to_columns": ["id"],
                    "on_delete": "c",
                    "on_update": "c"
                },
                {
                    "name": "fk_2",
                    "from_schema": "public",
                    "from_table": "t3",
                    "from_columns": ["id"],
                    "to_schema": "public",
                    "to_table": "t4",
                    "to_columns": ["id"],
                    "on_delete": "n",
                    "on_update": "d"
                }
            ]"#;

            let result = PostgresAdapter::parse_foreign_keys(json).unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].name, "fk_1");
            assert_eq!(result[0].on_delete, FkAction::Cascade);
            assert_eq!(result[1].name, "fk_2");
            assert_eq!(result[1].on_delete, FkAction::SetNull);
            assert_eq!(result[1].on_update, FkAction::SetDefault);
        }

        #[test]
        fn malformed_json_returns_invalid_json_error() {
            let result = PostgresAdapter::parse_foreign_keys("{not valid json}");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn missing_required_field_returns_error() {
            let json = r#"[{
                "name": "test_fk",
                "from_schema": "public",
                "from_table": "t1",
                "from_columns": ["id"],
                "to_schema": "public",
                "to_table": "t2",
                "to_columns": ["id"],
                "on_update": "a"
            }]"#;

            let result = PostgresAdapter::parse_foreign_keys(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn wrong_column_type_returns_error() {
            let json = r#"[{
                "name": "test_fk",
                "from_schema": "public",
                "from_table": "t1",
                "from_columns": "user_id",
                "to_schema": "public",
                "to_table": "t2",
                "to_columns": ["id"],
                "on_delete": "c",
                "on_update": "a"
            }]"#;

            let result = PostgresAdapter::parse_foreign_keys(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn empty_array_returns_empty_vec() {
            let json = r"[]";
            let result = PostgresAdapter::parse_foreign_keys(json).unwrap();
            assert!(result.is_empty());
        }
    }

    mod table_detail_combined_parsing {
        use super::*;

        fn build_combined_json(
            columns: &str,
            indexes: &str,
            fks: &str,
            rls: &str,
            triggers: &str,
            table_info: &str,
        ) -> String {
            format!(
                r#"{{
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
                r#"[{"name":"id","data_type":"integer","nullable":false,"default":null,"is_primary_key":true,"is_unique":false,"comment":null,"ordinal_position":1}]"#,
                "null",
                "null",
                r#"{"enabled":false,"force":false,"policies":[]}"#,
                "null",
                r#"{"owner":"postgres","comment":null,"row_count_estimate":42}"#,
            );

            let (columns, indexes, fks, rls, triggers, table_info) =
                PostgresAdapter::parse_table_detail_combined(&json).unwrap();

            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "id");
            assert!(indexes.is_empty());
            assert!(fks.is_empty());
            assert!(rls.is_some());
            assert!(!rls.unwrap().enabled);
            assert!(triggers.is_empty());
            assert_eq!(table_info.owner.as_deref(), Some("postgres"));
            assert_eq!(table_info.row_count_estimate, Some(42));
        }

        #[test]
        fn all_null_sub_values_parse_to_empty_defaults() {
            let json = build_combined_json("null", "null", "null", "null", "null", "null");

            let (columns, indexes, fks, rls, triggers, table_info) =
                PostgresAdapter::parse_table_detail_combined(&json).unwrap();

            assert!(columns.is_empty());
            assert!(indexes.is_empty());
            assert!(fks.is_empty());
            assert!(rls.is_none());
            assert!(triggers.is_empty());
            assert!(table_info.owner.is_none());
        }

        #[test]
        fn missing_key_returns_invalid_json_error() {
            let json = r#"{"columns": null, "indexes": null}"#;
            let result = PostgresAdapter::parse_table_detail_combined(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn unknown_key_returns_invalid_json_error() {
            let json = build_combined_json("null", "null", "null", "null", "null", "null")
                .replace('}', r#","extra_key": null}"#);
            let result = PostgresAdapter::parse_table_detail_combined(&json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn empty_input_returns_error() {
            let result = PostgresAdapter::parse_table_detail_combined("");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn null_input_returns_error() {
            let result = PostgresAdapter::parse_table_detail_combined("null");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }
    }

    mod table_columns_and_fks_parsing {
        use super::*;

        fn build_light_json(columns: &str, fks: &str) -> String {
            format!(r#"{{"columns": {columns}, "foreign_keys": {fks}}}"#)
        }

        #[test]
        fn valid_light_json_parses_columns_and_fks() {
            let json = build_light_json(
                r#"[{"name":"id","data_type":"integer","nullable":false,"default":null,"is_primary_key":true,"is_unique":false,"comment":null,"ordinal_position":1}]"#,
                r#"[{"name":"fk_1","from_schema":"public","from_table":"orders","from_columns":["user_id"],"to_schema":"public","to_table":"users","to_columns":["id"],"on_delete":"c","on_update":"a"}]"#,
            );

            let (columns, fks) = PostgresAdapter::parse_table_columns_and_fks(&json).unwrap();

            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "id");
            assert_eq!(fks.len(), 1);
            assert_eq!(fks[0].name, "fk_1");
        }

        #[test]
        fn null_sub_values_parse_to_empty() {
            let json = build_light_json("null", "null");

            let (columns, fks) = PostgresAdapter::parse_table_columns_and_fks(&json).unwrap();

            assert!(columns.is_empty());
            assert!(fks.is_empty());
        }

        #[test]
        fn missing_key_returns_error() {
            let json = r#"{"columns": null}"#;
            let result = PostgresAdapter::parse_table_columns_and_fks(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn unknown_key_returns_error() {
            let json = r#"{"columns": null, "foreign_keys": null, "extra": null}"#;
            let result = PostgresAdapter::parse_table_columns_and_fks(json);
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn empty_input_returns_error() {
            let result = PostgresAdapter::parse_table_columns_and_fks("");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }

        #[test]
        fn null_input_returns_error() {
            let result = PostgresAdapter::parse_table_columns_and_fks("null");
            assert!(matches!(result, Err(DbOperationError::InvalidJson(_))));
        }
    }
}
