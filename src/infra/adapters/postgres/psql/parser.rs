use crate::app::ports::MetadataError;
use crate::domain::{
    Column, CommandTag, FkAction, ForeignKey, Index, IndexType, RlsCommand, RlsInfo, RlsPolicy,
    Schema, TableSignature, TableSummary, Trigger, TriggerEvent, TriggerTiming,
};

use super::super::PostgresAdapter;

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
    ) -> Result<TableInfo, MetadataError> {
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

        let raw: RawTableInfo =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

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
    ) -> Result<Vec<TableSummary>, MetadataError> {
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

        let raw: Vec<RawTable> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        Ok(raw
            .into_iter()
            .map(|t| TableSummary::new(t.schema, t.name, t.row_count_estimate, t.has_rls))
            .collect())
    }

    pub(in crate::infra::adapters::postgres) fn parse_table_signatures(
        json: &str,
    ) -> Result<Vec<TableSignature>, MetadataError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(Vec::new());
        };

        #[derive(serde::Deserialize)]
        struct RawTableSignature {
            schema: String,
            name: String,
            signature: String,
        }

        let raw: Vec<RawTableSignature> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

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
    ) -> Result<Vec<Schema>, MetadataError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Ok(Vec::new());
        };

        #[derive(serde::Deserialize)]
        struct RawSchema {
            name: String,
        }

        let raw: Vec<RawSchema> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        Ok(raw.into_iter().map(|s| Schema::new(s.name)).collect())
    }

    pub(in crate::infra::adapters::postgres) fn parse_columns(
        json: &str,
    ) -> Result<Vec<Column>, MetadataError> {
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

        let raw: Vec<RawColumn> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

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
    ) -> Result<Vec<Index>, MetadataError> {
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

        let raw: Vec<RawIndex> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

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
    ) -> Result<Vec<ForeignKey>, MetadataError> {
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
                "a" => FkAction::NoAction,
                "r" => FkAction::Restrict,
                "c" => FkAction::Cascade,
                "n" => FkAction::SetNull,
                "d" => FkAction::SetDefault,
                _ => FkAction::NoAction,
            }
        }

        let raw: Vec<RawForeignKey> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

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
    ) -> Result<Option<RlsInfo>, MetadataError> {
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

        let raw: RawRls =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        let policies = raw
            .policies
            .into_iter()
            .map(|p| RlsPolicy {
                name: p.name,
                permissive: p.permissive,
                roles: p.roles.unwrap_or_default(),
                cmd: match p.cmd.as_str() {
                    "*" => RlsCommand::All,
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
    ) -> Result<Vec<Trigger>, MetadataError> {
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

        let raw: Vec<RawTrigger> =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

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
    ) -> Result<TableDetailCombined, MetadataError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Err(MetadataError::InvalidJson(
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

        let combined: CombinedDetail =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        let columns = Self::parse_columns(&combined.columns.to_string())?;
        let indexes = Self::parse_indexes(&combined.indexes.to_string())?;
        let foreign_keys = Self::parse_foreign_keys(&combined.foreign_keys.to_string())?;
        let rls = Self::parse_rls(&combined.rls.to_string())?;
        let triggers = Self::parse_triggers(&combined.triggers.to_string())?;
        let table_info = Self::parse_table_info(&combined.table_info.to_string())?;

        Ok((columns, indexes, foreign_keys, rls, triggers, table_info))
    }

    pub(in crate::infra::adapters::postgres) fn parse_table_detail_light(
        json: &str,
    ) -> Result<(Vec<Column>, Vec<ForeignKey>), MetadataError> {
        let Some(trimmed) = non_empty_json(json) else {
            return Err(MetadataError::InvalidJson(
                "table_detail_light: empty response".to_string(),
            ));
        };

        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct LightDetail {
            columns: serde_json::Value,
            foreign_keys: serde_json::Value,
        }

        let light: LightDetail =
            serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))?;

        let columns = Self::parse_columns(&light.columns.to_string())?;
        let foreign_keys = Self::parse_foreign_keys(&light.foreign_keys.to_string())?;

        Ok((columns, foreign_keys))
    }

    pub(in crate::infra::adapters::postgres) fn parse_command_tag(tag: &str) -> Option<CommandTag> {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return None;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        match parts.first().copied()? {
            "SELECT" => {
                let n = parts.get(1)?.parse::<u64>().ok()?;
                Some(CommandTag::Select(n))
            }
            "INSERT" => {
                // INSERT oid count — count is at index 2
                let n = parts.get(2)?.parse::<u64>().ok()?;
                Some(CommandTag::Insert(n))
            }
            "UPDATE" => {
                let n = parts.get(1)?.parse::<u64>().ok()?;
                Some(CommandTag::Update(n))
            }
            "DELETE" => {
                let n = parts.get(1)?.parse::<u64>().ok()?;
                Some(CommandTag::Delete(n))
            }
            "CREATE" => {
                let obj = parts.get(1)?;
                Some(CommandTag::Create(obj.to_string()))
            }
            "DROP" => {
                let obj = parts.get(1)?;
                Some(CommandTag::Drop(obj.to_string()))
            }
            "ALTER" => {
                let obj = parts.get(1)?;
                Some(CommandTag::Alter(obj.to_string()))
            }
            "TRUNCATE" => Some(CommandTag::Truncate),
            "BEGIN" => Some(CommandTag::Begin),
            "COMMIT" => Some(CommandTag::Commit),
            "ROLLBACK" => Some(CommandTag::Rollback),
            _ => Some(CommandTag::Other(trimmed.to_string())),
        }
    }

    pub(in crate::infra::adapters::postgres) fn extract_command_tag(
        stdout: &str,
    ) -> Option<CommandTag> {
        stdout
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .and_then(Self::parse_command_tag)
    }

    fn is_known_tcl_tag(s: &str) -> bool {
        matches!(
            s.split_whitespace().next().unwrap_or(""),
            "BEGIN" | "COMMIT" | "ROLLBACK" | "SAVEPOINT" | "RELEASE"
        ) || s == "START TRANSACTION"
    }

    fn parse_all_tags(stdout: &str) -> Option<Vec<CommandTag>> {
        let mut tags = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let tag = Self::parse_command_tag(trimmed)?;
            if let CommandTag::Other(ref raw) = tag
                && !Self::is_known_tcl_tag(raw)
            {
                return None;
            }
            tags.push(tag);
        }
        if tags.is_empty() {
            return None;
        }
        Some(tags)
    }

    fn discard_rolled_back(tags: &[CommandTag]) -> Vec<CommandTag> {
        let mut effective: Vec<CommandTag> = Vec::new();
        let mut frames: Vec<Vec<CommandTag>> = Vec::new();

        for tag in tags {
            match tag {
                CommandTag::Begin => {
                    frames.push(Vec::new());
                }
                CommandTag::Other(raw) if raw == "START TRANSACTION" => {
                    frames.push(Vec::new());
                }
                CommandTag::Other(raw) if raw == "SAVEPOINT" || raw.starts_with("SAVEPOINT ") => {
                    frames.push(Vec::new());
                }
                CommandTag::Other(raw) if raw == "RELEASE" || raw.starts_with("RELEASE ") => {
                    // Only merge a savepoint frame (depth > 1).
                    // At depth <= 1 the savepoint was already popped by ROLLBACK.
                    if frames.len() > 1
                        && let Some(inner) = frames.pop()
                    {
                        if let Some(parent) = frames.last_mut() {
                            parent.extend(inner);
                        } else {
                            effective.extend(inner);
                        }
                    }
                }
                CommandTag::Rollback => {
                    if frames.len() > 1 {
                        frames.pop();
                    } else {
                        frames.clear();
                    }
                }
                CommandTag::Commit => {
                    for frame in frames.drain(..) {
                        effective.extend(frame);
                    }
                }
                _ => {
                    if let Some(frame) = frames.last_mut() {
                        frame.push(tag.clone());
                    } else {
                        effective.push(tag.clone());
                    }
                }
            }
        }

        // Unclosed transaction: treat remaining frames as effective
        for frame in frames.drain(..) {
            effective.extend(frame);
        }

        effective
    }

    // -- CTAS / SELECT INTO correction (newspaper style: high→low) --

    pub(in crate::infra::adapters::postgres) fn parse_aggregate_command_tag(
        stdout: &str,
        sql: &str,
    ) -> Option<CommandTag> {
        ResolvedTags::resolve(stdout, sql)?.aggregate()
    }

    fn correct_ctas_tags(sql: &str, tags: Vec<CommandTag>) -> Vec<CommandTag> {
        let stmts = Self::split_sql_statements(sql);
        if stmts.len() != tags.len() {
            return tags;
        }
        tags.into_iter()
            .zip(stmts.iter())
            .map(|(tag, stmt)| {
                if !matches!(tag, CommandTag::Select(_)) {
                    return tag;
                }
                Self::detect_create_as_kind(stmt)
                    .or_else(|| Self::detect_select_into_kind(stmt))
                    .unwrap_or(tag)
            })
            .collect()
    }

    fn detect_create_as_kind(stmt: &str) -> Option<CommandTag> {
        let lower = stmt.trim().to_ascii_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        if words.first() != Some(&"create") {
            return None;
        }
        let mut idx = 1;
        while idx < words.len() && matches!(words[idx], "temp" | "temporary" | "unlogged") {
            idx += 1;
        }
        if idx < words.len() && words[idx] == "table" {
            return Some(CommandTag::Create("TABLE".to_string()));
        }
        if idx < words.len() && words[idx] == "materialized" && words.get(idx + 1) == Some(&"view")
        {
            return Some(CommandTag::Create("MATERIALIZED VIEW".to_string()));
        }
        None
    }

    fn detect_select_into_kind(stmt: &str) -> Option<CommandTag> {
        let lower = stmt.trim().to_ascii_lowercase();
        let first = lower.split_whitespace().next()?;
        if !matches!(first, "select" | "with") {
            return None;
        }
        Self::has_select_into(&lower).then(|| CommandTag::Create("TABLE".to_string()))
    }

    fn split_sql_statements(sql: &str) -> Vec<&str> {
        let bytes = sql.as_bytes();
        let mut stmts = Vec::new();
        let mut start = 0;
        let mut i = 0;

        while i < bytes.len() {
            match bytes[i] {
                b'\'' => i = skip_single_quoted(bytes, i),
                b'"' => i = skip_double_quoted(bytes, i),
                b'$' => i = skip_dollar_quoted(sql, bytes, i),
                b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                    i = skip_line_comment(bytes, i)
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                    i = skip_block_comment(bytes, i)
                }
                b';' => {
                    let slice = sql[start..i].trim();
                    if !slice.is_empty() {
                        stmts.push(slice);
                    }
                    i += 1;
                    start = i;
                }
                _ => i += 1,
            }
        }
        let tail = sql[start..].trim();
        if !tail.is_empty() {
            stmts.push(tail);
        }
        stmts
    }

    fn has_select_into(lower: &str) -> bool {
        let bytes = lower.as_bytes();
        let mut i = 0;
        let mut depth: i32 = 0;
        let mut found_from = false;

        while i < bytes.len() {
            match bytes[i] {
                b'\'' => i = skip_single_quoted(bytes, i),
                b'"' => i = skip_double_quoted(bytes, i),
                b'$' => i = skip_dollar_quoted(lower, bytes, i),
                b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                    i = skip_line_comment(bytes, i)
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                    i = skip_block_comment(bytes, i)
                }
                b'(' => {
                    depth += 1;
                    i += 1;
                }
                b')' => {
                    depth -= 1;
                    i += 1;
                }
                _ if depth == 0 && bytes[i].is_ascii_alphabetic() => {
                    let word_start = i;
                    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                    {
                        i += 1;
                    }
                    let word = &lower[word_start..i];
                    match word {
                        "from" => found_from = true,
                        "into" if !found_from => {
                            if word_start >= 6 && lower[..word_start].trim_end().ends_with("insert")
                            {
                                continue;
                            }
                            return true;
                        }
                        _ => {}
                    }
                }
                _ => i += 1,
            }
        }
        false
    }
}

#[cfg_attr(test, derive(Debug))]
pub(in crate::infra::adapters::postgres) struct ResolvedTags {
    all: Vec<CommandTag>,
    effective: Vec<CommandTag>,
}

impl ResolvedTags {
    pub(in crate::infra::adapters::postgres) fn resolve(stdout: &str, sql: &str) -> Option<Self> {
        let parsed = PostgresAdapter::parse_all_tags(stdout)?;
        let corrected = PostgresAdapter::correct_ctas_tags(sql, parsed);
        let effective = PostgresAdapter::discard_rolled_back(&corrected);
        Some(Self {
            all: corrected,
            effective,
        })
    }

    pub(in crate::infra::adapters::postgres) fn aggregate(&self) -> Option<CommandTag> {
        if let Some(tag) = self.effective.iter().find(|t| t.is_schema_modifying()) {
            return Some(tag.clone());
        }

        if let Some(tag) = self.effective.iter().rev().find(|t| t.needs_refresh()) {
            return Some(tag.clone());
        }

        if self.all.iter().any(|t| t.needs_refresh()) {
            return Some(CommandTag::Rollback);
        }

        self.all.last().cloned()
    }
}

// -- SQL lexical skip helpers (byte-level) --
// Shared by split_sql_statements and has_select_into to ensure
// consistent quote/comment boundary handling.

fn skip_single_quoted(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'\'' {
                i += 1;
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }
    i
}

fn skip_double_quoted(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'"' {
                i += 1;
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }
    i
}

fn skip_dollar_quoted(sql: &str, bytes: &[u8], mut i: usize) -> usize {
    let tag_start = i;
    i += 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'$' {
        let tag = &sql[tag_start..=i];
        i += 1;
        while i + tag.len() <= bytes.len() {
            if &sql[i..i + tag.len()] == tag {
                return i + tag.len();
            }
            i += 1;
        }
    }
    i
}

fn skip_line_comment(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_block_comment(bytes: &[u8], mut i: usize) -> usize {
    i += 2;
    while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
    }
    if i + 1 < bytes.len() {
        i += 2;
    }
    i
}

#[cfg(test)]
mod tests {
    use crate::app::ports::MetadataError;
    use crate::domain::CommandTag;
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
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn missing_field_returns_error() {
            let json = r#"[{"schema": "public", "name": "users"}]"#;
            let result = PostgresAdapter::parse_table_signatures(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
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
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
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
                    "cmd": "{}", "qual": null, "with_check": null
                }}]}}"#,
                cmd
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

            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }
    }

    mod json_parse_errors {
        use super::*;

        #[test]
        fn parse_tables_with_malformed_json_returns_error() {
            let result = PostgresAdapter::parse_tables("{not valid json}");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn parse_tables_with_wrong_structure_returns_error() {
            let result = PostgresAdapter::parse_tables(r#"["table1", "table2"]"#);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn parse_columns_with_missing_field_returns_error() {
            let json = r#"[{"name": "id", "nullable": true}]"#;
            let result = PostgresAdapter::parse_columns(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn parse_indexes_with_wrong_type_returns_error() {
            let json =
                r#"[{"name": "idx_test", "columns": "id", "unique": false, "primary": false}]"#;
            let result = PostgresAdapter::parse_indexes(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
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
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
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
                    "name": "test", "timing": "{}", "events": ["INSERT"],
                    "function_name": "func", "security_definer": false
                }}]"#,
                timing
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
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn empty_array_returns_empty_vec() {
            let json = r#"[]"#;
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
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn missing_name_field_returns_error() {
            let json = r#"[{}]"#;
            let result = PostgresAdapter::parse_schemas(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn wrong_structure_returns_error() {
            let json = r#"["public", "auth"]"#;
            let result = PostgresAdapter::parse_schemas(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn empty_array_returns_empty_vec() {
            let json = r#"[]"#;
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
                    "on_delete": "{}",
                    "on_update": "a"
                }}]"#,
                action_code
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
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
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
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
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
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn empty_array_returns_empty_vec() {
            let json = r#"[]"#;
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
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn unknown_key_returns_invalid_json_error() {
            let json = build_combined_json("null", "null", "null", "null", "null", "null")
                .replace("}", r#","extra_key": null}"#);
            let result = PostgresAdapter::parse_table_detail_combined(&json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn empty_input_returns_error() {
            let result = PostgresAdapter::parse_table_detail_combined("");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn null_input_returns_error() {
            let result = PostgresAdapter::parse_table_detail_combined("null");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }
    }

    mod table_detail_light_parsing {
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

            let (columns, fks) = PostgresAdapter::parse_table_detail_light(&json).unwrap();

            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "id");
            assert_eq!(fks.len(), 1);
            assert_eq!(fks[0].name, "fk_1");
        }

        #[test]
        fn null_sub_values_parse_to_empty() {
            let json = build_light_json("null", "null");

            let (columns, fks) = PostgresAdapter::parse_table_detail_light(&json).unwrap();

            assert!(columns.is_empty());
            assert!(fks.is_empty());
        }

        #[test]
        fn missing_key_returns_error() {
            let json = r#"{"columns": null}"#;
            let result = PostgresAdapter::parse_table_detail_light(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn unknown_key_returns_error() {
            let json = r#"{"columns": null, "foreign_keys": null, "extra": null}"#;
            let result = PostgresAdapter::parse_table_detail_light(json);
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn empty_input_returns_error() {
            let result = PostgresAdapter::parse_table_detail_light("");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }

        #[test]
        fn null_input_returns_error() {
            let result = PostgresAdapter::parse_table_detail_light("null");
            assert!(matches!(result, Err(MetadataError::InvalidJson(_))));
        }
    }

    mod split_sql_statements {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case::two_statements("SELECT 1; SELECT 2", vec!["SELECT 1", "SELECT 2"])]
        #[case::single_statement("SELECT 1", vec!["SELECT 1"])]
        #[case::trailing_semicolon("SELECT 1;", vec!["SELECT 1"])]
        #[case::multi_statement_txn(
            "BEGIN; CREATE TABLE t AS SELECT 1; COMMIT",
            vec!["BEGIN", "CREATE TABLE t AS SELECT 1", "COMMIT"]
        )]
        #[case::escaped_single_quote(
            "SELECT 'it''s'; SELECT 2",
            vec!["SELECT 'it''s'", "SELECT 2"]
        )]
        fn valid_input_returns_split_statements(#[case] sql: &str, #[case] expected: Vec<&str>) {
            assert_eq!(PostgresAdapter::split_sql_statements(sql), expected);
        }

        #[rstest]
        #[case::single_quotes("SELECT 'a;b'", vec!["SELECT 'a;b'"])]
        #[case::double_quotes(r#"SELECT "a;b""#, vec![r#"SELECT "a;b""#])]
        #[case::dollar_quote("SELECT $$a;b$$", vec!["SELECT $$a;b$$"])]
        #[case::tagged_dollar_quote("SELECT $tag$a;b$tag$", vec!["SELECT $tag$a;b$tag$"])]
        #[case::line_comment("SELECT 1 -- comment; here\n", vec!["SELECT 1 -- comment; here"])]
        #[case::block_comment("SELECT /* ; */ 1", vec!["SELECT /* ; */ 1"])]
        fn quoted_semicolon_returns_single_statement(
            #[case] sql: &str,
            #[case] expected: Vec<&str>,
        ) {
            assert_eq!(PostgresAdapter::split_sql_statements(sql), expected);
        }

        #[rstest]
        #[case::empty("")]
        #[case::whitespace_only("   ")]
        fn blank_input_returns_empty(#[case] sql: &str) {
            assert!(PostgresAdapter::split_sql_statements(sql).is_empty());
        }
    }

    mod detect_create_as_kind {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case::basic("CREATE TABLE t AS SELECT 1", "TABLE")]
        #[case::temp("CREATE TEMP TABLE t AS SELECT 1", "TABLE")]
        #[case::temporary("CREATE TEMPORARY TABLE t AS SELECT 1", "TABLE")]
        #[case::unlogged("CREATE UNLOGGED TABLE t AS SELECT 1", "TABLE")]
        #[case::lowercase("create table t as select 1", "TABLE")]
        #[case::materialized_view("CREATE MATERIALIZED VIEW v AS SELECT 1", "MATERIALIZED VIEW")]
        fn create_as_returns_correct_tag(#[case] stmt: &str, #[case] object: &str) {
            assert_eq!(
                PostgresAdapter::detect_create_as_kind(stmt),
                Some(CommandTag::Create(object.to_string()))
            );
        }

        #[rstest]
        #[case::create_view("CREATE VIEW v AS SELECT 1")]
        #[case::plain_select("SELECT * FROM users")]
        fn non_ctas_returns_none(#[case] stmt: &str) {
            assert_eq!(PostgresAdapter::detect_create_as_kind(stmt), None);
        }
    }

    mod detect_select_into_kind {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case::basic("SELECT * INTO t FROM users")]
        #[case::with_columns("SELECT id, name INTO t FROM users")]
        #[case::with_cte("WITH cte AS (SELECT 1) SELECT * INTO t FROM cte")]
        fn select_into_returns_create_table(#[case] stmt: &str) {
            assert_eq!(
                PostgresAdapter::detect_select_into_kind(stmt),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[rstest]
        #[case::plain_select("SELECT * FROM users")]
        #[case::insert_into("INSERT INTO users VALUES (1)")]
        #[case::create_table("CREATE TABLE t AS SELECT 1")]
        fn non_select_into_returns_none(#[case] stmt: &str) {
            assert_eq!(PostgresAdapter::detect_select_into_kind(stmt), None);
        }

        #[rstest]
        #[case::double_quotes(r#"SELECT "into" FROM t"#)]
        #[case::dollar_quote("SELECT $$into$$ FROM t")]
        #[case::tagged_dollar_quote("SELECT $x$into$x$ FROM t")]
        #[case::line_comment("SELECT 1 -- into\nFROM t")]
        #[case::block_comment("SELECT /* into */ 1 FROM t")]
        #[case::single_quote("SELECT 'into' FROM t")]
        fn quoted_into_returns_none(#[case] stmt: &str) {
            assert_eq!(PostgresAdapter::detect_select_into_kind(stmt), None);
        }
    }

    mod command_tag_parsing {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("SELECT 5", CommandTag::Select(5))]
        #[case("SELECT 0", CommandTag::Select(0))]
        #[case("INSERT 0 3", CommandTag::Insert(3))]
        #[case("INSERT 0 0", CommandTag::Insert(0))]
        #[case("UPDATE 5", CommandTag::Update(5))]
        #[case("UPDATE 0", CommandTag::Update(0))]
        #[case("DELETE 10", CommandTag::Delete(10))]
        #[case("DELETE 0", CommandTag::Delete(0))]
        #[case("CREATE TABLE", CommandTag::Create("TABLE".to_string()))]
        #[case("CREATE INDEX", CommandTag::Create("INDEX".to_string()))]
        #[case("DROP TABLE", CommandTag::Drop("TABLE".to_string()))]
        #[case("DROP INDEX", CommandTag::Drop("INDEX".to_string()))]
        #[case("ALTER TABLE", CommandTag::Alter("TABLE".to_string()))]
        #[case("TRUNCATE TABLE", CommandTag::Truncate)]
        #[case("BEGIN", CommandTag::Begin)]
        #[case("COMMIT", CommandTag::Commit)]
        #[case("ROLLBACK", CommandTag::Rollback)]
        fn parse_known_tags(#[case] input: &str, #[case] expected: CommandTag) {
            assert_eq!(PostgresAdapter::parse_command_tag(input), Some(expected));
        }

        #[rstest]
        #[case("")]
        #[case("   ")]
        fn empty_or_whitespace_returns_none(#[case] input: &str) {
            assert_eq!(PostgresAdapter::parse_command_tag(input), None);
        }

        #[rstest]
        #[case("SELECT abc")]
        #[case("INSERT 0")]
        #[case("INSERT 0 abc")]
        #[case("UPDATE")]
        #[case("DELETE")]
        fn malformed_count_returns_none(#[case] input: &str) {
            assert_eq!(PostgresAdapter::parse_command_tag(input), None);
        }

        #[test]
        fn unknown_command_returns_other() {
            assert_eq!(
                PostgresAdapter::parse_command_tag("VACUUM"),
                Some(CommandTag::Other("VACUUM".to_string()))
            );
        }

        #[test]
        fn extract_from_multiline_with_notice() {
            let stdout = "NOTICE:  table \"foo\" does not exist, skipping\nDROP TABLE\n";
            assert_eq!(
                PostgresAdapter::extract_command_tag(stdout),
                Some(CommandTag::Drop("TABLE".to_string()))
            );
        }

        #[test]
        fn extract_skips_trailing_empty_lines() {
            let stdout = "INSERT 0 7\n\n  \n";
            assert_eq!(
                PostgresAdapter::extract_command_tag(stdout),
                Some(CommandTag::Insert(7))
            );
        }

        #[test]
        fn extract_from_empty_returns_none() {
            assert_eq!(PostgresAdapter::extract_command_tag(""), None);
            assert_eq!(PostgresAdapter::extract_command_tag("  \n  \n"), None);
        }

        #[test]
        fn parse_affected_rows_regression() {
            assert_eq!(PostgresAdapter::parse_affected_rows("UPDATE 3\n"), Some(3));
            assert_eq!(PostgresAdapter::parse_affected_rows("DELETE 5\n"), Some(5));
            assert_eq!(
                PostgresAdapter::parse_affected_rows("INSERT 0 10\n"),
                Some(10)
            );
            assert_eq!(PostgresAdapter::parse_affected_rows("SELECT 1\n"), Some(1));
        }
    }

    mod is_known_tcl_tag {
        use super::*;

        #[test]
        fn recognizes_begin_commit_rollback() {
            assert!(PostgresAdapter::is_known_tcl_tag("BEGIN"));
            assert!(PostgresAdapter::is_known_tcl_tag("COMMIT"));
            assert!(PostgresAdapter::is_known_tcl_tag("ROLLBACK"));
        }

        #[test]
        fn recognizes_savepoint_and_release() {
            assert!(PostgresAdapter::is_known_tcl_tag("SAVEPOINT"));
            assert!(PostgresAdapter::is_known_tcl_tag("RELEASE"));
        }

        #[test]
        fn recognizes_start_transaction() {
            assert!(PostgresAdapter::is_known_tcl_tag("START TRANSACTION"));
        }

        #[test]
        fn rejects_non_tcl() {
            assert!(!PostgresAdapter::is_known_tcl_tag("UPDATE 1"));
            assert!(!PostgresAdapter::is_known_tcl_tag("id,name"));
            assert!(!PostgresAdapter::is_known_tcl_tag("VACUUM"));
        }
    }

    mod parse_all_tags {
        use super::*;

        #[test]
        fn single_dml() {
            assert_eq!(
                PostgresAdapter::parse_all_tags("DELETE 3"),
                Some(vec![CommandTag::Delete(3)])
            );
        }

        #[test]
        fn single_ddl() {
            assert_eq!(
                PostgresAdapter::parse_all_tags("CREATE TABLE"),
                Some(vec![CommandTag::Create("TABLE".to_string())])
            );
        }

        #[test]
        fn multi_line_tags() {
            assert_eq!(
                PostgresAdapter::parse_all_tags("BEGIN\nUPDATE 1\nCOMMIT"),
                Some(vec![
                    CommandTag::Begin,
                    CommandTag::Update(1),
                    CommandTag::Commit,
                ])
            );
        }

        #[test]
        fn csv_returns_none() {
            assert_eq!(PostgresAdapter::parse_all_tags("id,name\n1,Alice"), None);
        }

        #[test]
        fn empty_string_returns_none() {
            assert_eq!(PostgresAdapter::parse_all_tags(""), None);
        }

        #[test]
        fn skips_empty_lines() {
            assert_eq!(
                PostgresAdapter::parse_all_tags("BEGIN\n\nUPDATE 1\n"),
                Some(vec![CommandTag::Begin, CommandTag::Update(1)])
            );
        }

        #[test]
        fn savepoint_line_is_accepted() {
            let result = PostgresAdapter::parse_all_tags("SAVEPOINT");
            assert!(result.is_some());
            assert_eq!(
                result.unwrap(),
                vec![CommandTag::Other("SAVEPOINT".to_string())]
            );
        }
    }

    mod discard_rolled_back {
        use super::*;

        fn sp() -> CommandTag {
            CommandTag::Other("SAVEPOINT".to_string())
        }
        fn release() -> CommandTag {
            CommandTag::Other("RELEASE".to_string())
        }

        #[test]
        fn committed_txn() {
            let tags = vec![CommandTag::Begin, CommandTag::Update(1), CommandTag::Commit];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1)]
            );
        }

        #[test]
        fn full_rollback() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                CommandTag::Rollback,
            ];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn no_txn() {
            let tags = vec![CommandTag::Update(1)];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1)]
            );
        }

        #[test]
        fn savepoint_release() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                sp(),
                CommandTag::Insert(1),
                release(),
                CommandTag::Commit,
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1), CommandTag::Insert(1)]
            );
        }

        #[test]
        fn partial_rollback() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                sp(),
                CommandTag::Insert(1),
                CommandTag::Rollback,
                CommandTag::Commit,
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1)]
            );
        }

        #[test]
        fn full_rollback_with_savepoint() {
            let tags = vec![
                CommandTag::Begin,
                sp(),
                CommandTag::Create("TABLE".to_string()),
                CommandTag::Rollback,
                CommandTag::Commit,
            ];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn unclosed_txn() {
            let tags = vec![CommandTag::Begin, CommandTag::Update(1)];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1)]
            );
        }

        #[test]
        fn tcl_only() {
            let tags = vec![CommandTag::Begin, CommandTag::Commit];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn rollback_then_dml() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                sp(),
                CommandTag::Insert(1),
                CommandTag::Rollback,
                CommandTag::Delete(3),
                CommandTag::Commit,
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1), CommandTag::Delete(3)]
            );
        }

        #[test]
        fn rollback_then_release_same_sp() {
            let tags = vec![
                CommandTag::Begin,
                sp(),
                CommandTag::Insert(1),
                CommandTag::Rollback,
                release(),
                CommandTag::Commit,
            ];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn release_after_rollback_does_not_leak_outer_frame() {
            // C2 regression: RELEASE after ROLLBACK must not pop the
            // transaction frame and leak its contents into effective.
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                sp(),
                CommandTag::Insert(1),
                CommandTag::Rollback,
                release(),
                CommandTag::Rollback,
            ];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn multiple_txns() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                CommandTag::Commit,
                CommandTag::Begin,
                CommandTag::Insert(1),
                CommandTag::Commit,
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1), CommandTag::Insert(1)]
            );
        }

        #[test]
        fn full_rollback_then_bare_dml() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                CommandTag::Rollback,
                CommandTag::Delete(3),
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Delete(3)]
            );
        }
    }

    mod resolved_tags_aggregate {
        use super::*;
        use crate::infra::adapters::postgres::psql::parser::ResolvedTags;

        fn resolved(all: Vec<CommandTag>, effective: Vec<CommandTag>) -> ResolvedTags {
            ResolvedTags { all, effective }
        }

        #[test]
        fn schema_modifying_takes_priority() {
            let tags = vec![CommandTag::Drop("TABLE".to_string()), CommandTag::Delete(1)];
            assert_eq!(
                resolved(tags.clone(), tags).aggregate(),
                Some(CommandTag::Drop("TABLE".to_string()))
            );
        }

        #[test]
        fn last_needs_refresh() {
            let tags = vec![CommandTag::Insert(1), CommandTag::Update(2)];
            assert_eq!(
                resolved(tags.clone(), tags).aggregate(),
                Some(CommandTag::Update(2))
            );
        }

        #[test]
        fn effective_empty_with_modifying_returns_rollback() {
            let all = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                CommandTag::Rollback,
            ];
            assert_eq!(
                resolved(all, vec![]).aggregate(),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn tcl_only_returns_last_tag() {
            let all = vec![CommandTag::Begin, CommandTag::Commit];
            assert_eq!(resolved(all, vec![]).aggregate(), Some(CommandTag::Commit));
        }

        #[test]
        fn ddl_and_dml_mixed_schema_wins() {
            let tags = vec![
                CommandTag::Create("TABLE".to_string()),
                CommandTag::Insert(1),
            ];
            assert_eq!(
                resolved(tags.clone(), tags).aggregate(),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }
    }

    mod parse_aggregate_command_tag {
        use super::*;

        #[test]
        fn committed_txn_with_update() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nUPDATE 1\nCOMMIT",
                    "BEGIN; UPDATE t SET x=1; COMMIT"
                ),
                Some(CommandTag::Update(1))
            );
        }

        #[test]
        fn rolled_back_txn() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nUPDATE 1\nROLLBACK",
                    "BEGIN; UPDATE t SET x=1; ROLLBACK"
                ),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn partial_rollback_keeps_outer_dml() {
            let stdout = "BEGIN\nUPDATE 1\nSAVEPOINT\nINSERT 0 1\nROLLBACK\nCOMMIT";
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    stdout,
                    "BEGIN; UPDATE t SET x=1; SAVEPOINT s; INSERT INTO t VALUES(1); ROLLBACK TO SAVEPOINT s; COMMIT"
                ),
                Some(CommandTag::Update(1))
            );
        }

        #[test]
        fn single_dml() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag("DELETE 3", "DELETE FROM t"),
                Some(CommandTag::Delete(3))
            );
        }

        #[test]
        fn csv_returns_none() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag("id,name", "SELECT * FROM t"),
                None
            );
        }

        #[test]
        fn single_line_select_passes_through() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag("SELECT 5", "SELECT 1+4"),
                Some(CommandTag::Select(5))
            );
        }

        #[test]
        fn ctas_committed() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSELECT 1\nCOMMIT",
                    "BEGIN; CREATE TABLE t AS SELECT 1; COMMIT"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn ctas_savepoint_rollback_with_outer_dml() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSAVEPOINT\nSELECT 1\nROLLBACK\nUPDATE 1\nCOMMIT",
                    "BEGIN; SAVEPOINT s; CREATE TABLE t AS SELECT 1; ROLLBACK TO SAVEPOINT s; UPDATE t SET x=1; COMMIT"
                ),
                Some(CommandTag::Update(1))
            );
        }

        #[test]
        fn ctas_savepoint_rollback_no_other_dml() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSAVEPOINT\nSELECT 1\nROLLBACK\nCOMMIT",
                    "BEGIN; SAVEPOINT s; CREATE TABLE t AS SELECT 1; ROLLBACK TO SAVEPOINT s; COMMIT"
                ),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn select_into_savepoint_rollback() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSAVEPOINT\nSELECT 5\nROLLBACK\nCOMMIT",
                    "BEGIN; SAVEPOINT s; SELECT * INTO t FROM u; ROLLBACK TO SAVEPOINT s; COMMIT"
                ),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn single_ctas_no_txn() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1",
                    "CREATE TABLE t AS SELECT 1"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn ctas_outside_savepoint_survives() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSELECT 1\nCOMMIT",
                    "BEGIN; CREATE TABLE t AS SELECT 1; COMMIT"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn ctas_full_rollback() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSELECT 1\nROLLBACK",
                    "BEGIN; CREATE TABLE t AS SELECT 1; ROLLBACK"
                ),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn create_temp_table_as() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1",
                    "CREATE TEMP TABLE t AS SELECT 1"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn create_materialized_view() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1",
                    "CREATE MATERIALIZED VIEW v AS SELECT 1"
                ),
                Some(CommandTag::Create("MATERIALIZED VIEW".to_string()))
            );
        }

        #[test]
        fn with_select_into() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1",
                    "WITH cte AS (SELECT 1) SELECT * INTO t FROM cte"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn ctas_then_delete_no_txn() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1\nDELETE 1",
                    "CREATE TABLE t AS SELECT 1; DELETE FROM users WHERE id = 1"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn select_into_then_delete_no_txn() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 5\nDELETE 1",
                    "SELECT * INTO t FROM users; DELETE FROM users WHERE id = 1"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn count_mismatch_fallback() {
            // 1 statement but 2 tags → skip correction, use last tag
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag("SELECT 1\nSELECT 2", "SELECT 1"),
                Some(CommandTag::Select(2))
            );
        }
    }
}
