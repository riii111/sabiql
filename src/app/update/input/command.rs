use crate::update::action::{Action, ModalKind};

pub fn action_for_command(input: &str) -> Action {
    match input.trim() {
        "q" | "quit" => Action::Quit,
        "?" | "help" => Action::ToggleModal(ModalKind::Help),
        "sql" => Action::OpenModal(ModalKind::SqlModal),
        "erd" => Action::OpenModal(ModalKind::ErTablePicker),
        "settings" | "theme" => Action::OpenModal(ModalKind::Settings),
        "palette" => Action::OpenModal(ModalKind::CommandPalette),
        "w" | "write" => Action::SubmitCellEditWrite,
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod action_for_command {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("q")]
        #[case("quit")]
        fn quit_aliases(#[case] input: &str) {
            assert!(matches!(action_for_command(input), Action::Quit));
        }

        #[rstest]
        #[case("?")]
        #[case("help")]
        fn help_aliases(#[case] input: &str) {
            assert!(matches!(
                action_for_command(input),
                Action::ToggleModal(ModalKind::Help)
            ));
        }

        #[test]
        fn sql_opens_sql_modal() {
            assert!(matches!(
                action_for_command("sql"),
                Action::OpenModal(ModalKind::SqlModal)
            ));
        }

        #[test]
        fn erd_opens_er_table_picker() {
            assert!(matches!(
                action_for_command("erd"),
                Action::OpenModal(ModalKind::ErTablePicker)
            ));
        }

        #[rstest]
        #[case("settings")]
        #[case("theme")]
        fn settings_aliases_open_settings(#[case] input: &str) {
            assert!(matches!(
                action_for_command(input),
                Action::OpenModal(ModalKind::Settings)
            ));
        }

        #[test]
        fn palette_opens_command_palette() {
            assert!(matches!(
                action_for_command("palette"),
                Action::OpenModal(ModalKind::CommandPalette)
            ));
        }

        #[rstest]
        #[case("w")]
        #[case("write")]
        fn write_aliases(#[case] input: &str) {
            assert!(matches!(
                action_for_command(input),
                Action::SubmitCellEditWrite
            ));
        }

        #[rstest]
        #[case("foo")]
        #[case("5")]
        #[case("  42  ")]
        #[case("42foo")]
        #[case("")]
        fn unknown_commands_return_none(#[case] input: &str) {
            assert!(matches!(action_for_command(input), Action::None));
        }

        #[test]
        fn whitespace_is_trimmed() {
            assert!(matches!(
                action_for_command("  sql  "),
                Action::OpenModal(ModalKind::SqlModal)
            ));
        }
    }
}
