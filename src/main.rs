use anyhow::Result;
use bom_buddy::cli::cli;

fn main() -> Result<()> {
    cli()?;
    Ok(())
}
