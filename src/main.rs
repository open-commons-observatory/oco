//! oco: the Open Commons Observatory's face - its website, rendered from the data ocoeco gathers.
//!
//!   oco site --data DIR --out DIR

mod site;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    version,
    about = "The Open Commons Observatory's face: its website, from ocoeco's data"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render the observatory's website from a harvested data folder
    Site {
        #[arg(long)]
        data: std::path::PathBuf,
        #[arg(long)]
        out: std::path::PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Site { data, out } => {
            site::render(&site::Data::load(&data)?, &out)?;
            println!("site -> {}", out.display());
        }
    }
    Ok(())
}
