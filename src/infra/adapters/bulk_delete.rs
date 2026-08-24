use crate::domain::QueryValue;

pub(super) fn rows_predicate(
    pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    equality_predicate: impl Fn(&str, &QueryValue) -> String,
) -> String {
    let predicates = pk_pairs_per_row
        .iter()
        .map(|pairs| {
            pairs
                .iter()
                .map(|(column, value)| equality_predicate(column, value))
                .collect::<Vec<_>>()
                .join(" AND ")
        })
        .collect::<Vec<_>>();
    if predicates.len() == 1 {
        predicates[0].clone()
    } else {
        predicates
            .into_iter()
            .map(|predicate| format!("({predicate})"))
            .collect::<Vec<_>>()
            .join(" OR ")
    }
}
