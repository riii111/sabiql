pub mod adapter_postgres;
pub mod harness;

use clap::Parser;

use super::{Args, Command, self_update_disabled_message};

#[test]
fn no_subcommand_returns_none() {
    let args = Args::parse_from(["sabiql"]);
    assert!(args.command.is_none());
}

#[test]
fn update_subcommand_is_recognized() {
    let args = Args::parse_from(["sabiql", "update"]);
    assert!(matches!(args.command, Some(Command::Update)));
}

#[test]
#[cfg(not(feature = "self-update"))]
fn disabled_message_contains_version_and_upgrade_guidance() {
    let msg = self_update_disabled_message();
    assert!(msg.contains(env!("CARGO_PKG_VERSION")));
    assert!(msg.contains("brew upgrade sabiql"));
    assert!(msg.contains("cargo install sabiql"));
}
