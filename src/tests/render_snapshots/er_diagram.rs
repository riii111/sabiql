use super::*;
use harness::explorer_selected_state;
use sabiql_app::model::app_state::AppState;
use sabiql_app::model::er_state::ErDiagram;
use sabiql_app::ports::outbound::ErDiagramLayout;
use sabiql_domain::ErTableInfo;
use sabiql_domain::er::{ErFkInfo, fk_reachable_tables_multi};
use sabiql_ui::adapters::RataflowErLayout;

#[test]
fn er_waiting_progress() {
    let mut state = explorer_selected_state();
    let mut terminal = create_test_terminal();

    let _ = state.er_preparation.start_waiting_run();
    state.er_preparation.begin_all_prefetch([
        "public.users".to_string(),
        "public.comments".to_string(),
        "public.posts".to_string(),
    ]);
    state
        .table_prefetch
        .queue_table_prefetch("public.comments".to_string());
    state
        .table_prefetch
        .start_table_prefetch("public.posts".to_string());

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_modal() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_filtered() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state.ui.er_picker_mut().insert_filter_str("user");

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_single_select() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state
        .ui
        .replace_er_selected_tables(["public.users".to_string()]);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_multi_select() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state
        .ui
        .replace_er_selected_tables(["public.users".to_string(), "public.posts".to_string()]);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_table_picker_all_selected() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);
    state.ui.replace_er_selected_tables([
        "public.users".to_string(),
        "public.posts".to_string(),
        "public.comments".to_string(),
    ]);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn mysql_er_table_picker_shows_table_names_without_database() {
    let mut state = super::harness::create_test_state();
    state.session.activate_connection_with_target(
        &sabiql_domain::ConnectionId::from_string("mysql-test"),
        "mysql",
        sabiql_domain::DatabaseType::MySQL,
        "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
        Some("app"),
    );
    state.session.mark_connected(std::sync::Arc::new(
        super::harness::fixtures::sample_metadata(),
    ));
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::ErTablePicker);

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn mysql_er_table_picker_single_select_shows_table_name_without_database() {
    let mut state = super::harness::create_test_state();
    state.session.activate_connection_with_target(
        &sabiql_domain::ConnectionId::from_string("mysql-test"),
        "mysql",
        sabiql_domain::DatabaseType::MySQL,
        "mysql://user@localhost:3306/app?ssl-mode=PREFERRED",
        Some("app"),
    );
    let mut metadata = super::harness::fixtures::sample_metadata();
    for table in &mut metadata.table_summaries {
        table.schema = "app".to_string();
    }
    state.session.mark_connected(std::sync::Arc::new(metadata));
    state.modal.set_mode(InputMode::ErTablePicker);
    state
        .ui
        .replace_er_selected_tables(["app.users".to_string()]);
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

fn er_table(schema: &str, name: &str, fks: &[(&str, &str)]) -> ErTableInfo {
    let qualified_name = format!("{schema}.{name}");
    ErTableInfo {
        foreign_keys: fks
            .iter()
            .map(|(fk_name, to)| ErFkInfo {
                name: (*fk_name).to_string(),
                from_qualified: qualified_name.clone(),
                to_qualified: (*to).to_string(),
            })
            .collect(),
        qualified_name,
        name: name.to_string(),
        schema: schema.to_string(),
    }
}

fn er_viewer_state(tables: Vec<ErTableInfo>, seed_tables: &[String]) -> AppState {
    let mut state = explorer_selected_state();
    let total_tables = tables.len();
    let tables = if seed_tables.is_empty() {
        tables
    } else {
        fk_reachable_tables_multi(&tables, seed_tables, 1)
    };
    let layout = RataflowErLayout.layout(&tables, seed_tables);
    state.er_diagram = Some(ErDiagram {
        id: 1,
        tables,
        total_tables,
        layout,
    });
    state.modal.push_mode(InputMode::ErDiagram);
    state
}

fn er_sample_tables() -> Vec<ErTableInfo> {
    vec![
        er_table("public", "users", &[]),
        er_table(
            "public",
            "posts",
            &[
                ("fk_posts_author", "public.users"),
                ("fk_posts_editor", "public.users"),
            ],
        ),
        er_table(
            "public",
            "comments",
            &[
                ("fk_comments_post", "public.posts"),
                ("fk_comments_parent", "public.comments"),
            ],
        ),
        er_table("public", "teams", &[("fk_teams_lead", "public.members")]),
        er_table("public", "members", &[("fk_members_team", "public.teams")]),
        er_table("audit", "logs", &[]),
    ]
}

#[test]
fn er_viewer_lays_out_cycles_self_references_and_parallel_fks() {
    let mut state = er_viewer_state(er_sample_tables(), &[]);
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}

#[test]
fn er_viewer_partial_diagram_selects_seed_and_labels_its_fks() {
    let mut state = er_viewer_state(er_sample_tables(), &["public.posts".to_string()]);
    let mut terminal = create_test_terminal();

    let output = render_to_string(&mut terminal, &mut state);

    insta::assert_snapshot!(output);
}
