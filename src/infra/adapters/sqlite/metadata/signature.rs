use crate::domain::{Table, TableSignature};

use super::super::schema::MAIN_SCHEMA;

pub(super) fn signature_for_table(detail: &Table) -> TableSignature {
    let kind_info = &detail.kind_info;
    let mut parts = vec![
        format!("sql={}", detail.source_ddl.clone().unwrap_or_default()),
        format!("kind={:?}", kind_info.kind),
        format!("strict={}", kind_info.is_strict),
        format!("wr={}", kind_info.without_rowid),
        format!(
            "module={}",
            kind_info.virtual_module.as_deref().unwrap_or_default()
        ),
    ];
    parts.extend(detail.columns.iter().map(|column| {
        format!(
            "col={}:{}:{}:{}:{}:{}:{}",
            column.name,
            column.data_type,
            column.is_nullable(),
            column.default.clone().unwrap_or_default(),
            column.is_read_only(),
            column.is_hidden(),
            column.is_generated()
        )
    }));
    parts.extend(detail.indexes.iter().map(|index| {
        format!(
            "idx={}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            index.name,
            index.columns.join(","),
            index.is_unique(),
            index.is_primary(),
            index.is_partial(),
            index.has_expression(),
            index.has_auxiliary_columns(),
            index.has_descending_key(),
            index.has_non_binary_collation(),
            index.definition.clone().unwrap_or_default()
        )
    }));
    parts.extend(detail.foreign_keys.iter().map(|fk| {
        format!(
            "fk={}:{}:{}:{}:{}:{}:{}",
            fk.name,
            fk.from_columns.join(","),
            fk.to_table,
            fk.to_columns.join(","),
            fk.on_delete,
            fk.on_update,
            fk.reference_resolved
        )
    }));
    parts.extend(detail.triggers.iter().map(|trigger| {
        format!(
            "trg={}:{}:{}:{}",
            trigger.name,
            trigger.timing,
            trigger
                .events
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
            trigger.definition
        )
    }));

    TableSignature {
        schema: MAIN_SCHEMA.to_string(),
        name: detail.name.clone(),
        signature: parts.join("|"),
    }
}

#[cfg(test)]
#[path = "signature_tests.rs"]
mod tests;
