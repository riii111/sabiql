mod error;

use color_eyre::Result;

fn main() -> Result<()> {
    error::install_hooks()?;
    println!("Hello, dbtui!");
    Ok(())
}
