use std::sync::Arc;

use tokio::sync::mpsc;

use crate::domain::ErTableInfo;
use crate::ports::outbound::ErDiagramLayout;
use crate::update::action::{Action, ErDiagramInfo};

pub(in crate::cmd) fn spawn_er_layout_task(
    layout_engine: Arc<dyn ErDiagramLayout>,
    tables: Vec<ErTableInfo>,
    seed_tables: Vec<String>,
    run_id: u64,
    total_tables: usize,
    tx: mpsc::Sender<Action>,
) {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            let layout = layout_engine.layout(&tables, &seed_tables);
            (tables, layout)
        })
        .await;

        let action = match result {
            Ok((tables, layout)) => Action::ErDiagramOpened(ErDiagramInfo {
                run_id,
                tables,
                layout,
                total_tables,
            }),
            Err(e) => Action::ErDiagramFailed {
                run_id,
                error: format!("Task panicked: {e}"),
            },
        };
        let _ = tx.send(action).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::test_fixtures::recv_action_with_timeout;
    use crate::domain::er::ErFkInfo;
    use crate::model::er_state::ErLayout;
    use std::time::Duration;

    struct FixedLayout;
    impl ErDiagramLayout for FixedLayout {
        fn layout(&self, tables: &[ErTableInfo], seed_tables: &[String]) -> ErLayout {
            ErLayout {
                positions: tables
                    .iter()
                    .map(|t| (t.qualified_name.clone(), (1.0, 2.0)))
                    .collect(),
                focus_tables: seed_tables.to_vec(),
                selected_table: None,
            }
        }
    }

    struct PanicLayout;
    impl ErDiagramLayout for PanicLayout {
        fn layout(&self, _tables: &[ErTableInfo], _seed_tables: &[String]) -> ErLayout {
            panic!("intentional panic")
        }
    }

    fn users_table() -> ErTableInfo {
        ErTableInfo {
            qualified_name: "public.users".to_string(),
            name: "users".to_string(),
            schema: "public".to_string(),
            foreign_keys: vec![ErFkInfo {
                name: "fk_users_org".to_string(),
                from_qualified: "public.users".to_string(),
                to_qualified: "public.orgs".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn success_sends_opened_action_with_layout() {
        let (tx, mut rx) = mpsc::channel(1);

        spawn_er_layout_task(
            Arc::new(FixedLayout),
            vec![users_table()],
            vec!["public.users".to_string()],
            3,
            5,
            tx,
        );

        let action = recv_action_with_timeout(&mut rx, Duration::from_secs(1)).await;
        match action {
            Action::ErDiagramOpened(ErDiagramInfo {
                run_id,
                tables,
                layout,
                total_tables,
            }) => {
                assert_eq!(run_id, 3);
                assert_eq!(total_tables, 5);
                assert_eq!(tables.len(), 1);
                assert_eq!(layout.positions.get("public.users"), Some(&(1.0, 2.0)));
                assert_eq!(layout.focus_tables, vec!["public.users"]);
            }
            _ => panic!("expected ErDiagramOpened, got {action:?}"),
        }
    }

    #[tokio::test]
    async fn panic_sends_failed_action() {
        let (tx, mut rx) = mpsc::channel(1);

        spawn_er_layout_task(
            Arc::new(PanicLayout),
            vec![users_table()],
            vec![],
            11,
            5,
            tx,
        );

        let action = recv_action_with_timeout(&mut rx, Duration::from_secs(1)).await;
        match action {
            Action::ErDiagramFailed { run_id, error } => {
                assert!(error.contains("Task panicked"));
                assert_eq!(run_id, 11);
            }
            _ => panic!("expected ErDiagramFailed, got {action:?}"),
        }
    }
}
