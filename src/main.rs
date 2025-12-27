mod app;
mod domain;
mod error;
mod infra;
mod ui;

use clap::Parser;
use color_eyre::eyre::Result;

use infra::config::{
    cache::get_cache_dir,
    dbx_toml::DbxConfig,
    project_root::{find_project_root, get_project_name},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "default")]
    profile: String,
}

fn main() -> Result<()> {
    error::install_hooks()?;

    let args = Args::parse();
    let project_root = find_project_root()?;
    let project_name = get_project_name(&project_root);

    let config_path = project_root.join(".dbx.toml");
    let config = if config_path.exists() {
        Some(DbxConfig::load(&config_path)?)
    } else {
        None
    };

    let dsn = config.as_ref().and_then(|c| c.resolve_dsn(&args.profile));
    let _cache_dir = get_cache_dir(&project_name)?;

    println!("Project: {}", project_name);
    println!("Profile: {}", args.profile);
    println!("DSN: {:?}", dsn);

    Ok(())
}
