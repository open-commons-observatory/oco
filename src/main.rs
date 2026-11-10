//! oco: the Open Commons Observatory's engine.
//!
//!   oco discover [--kind KIND] [--json]   every marked project the outlets find, each confirmed by its mark

mod discover;
mod mark;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "The Open Commons Observatory's engine")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Find every marked project (GitHub: the oco-project topic), each confirmed by its oco.toml
    Discover {
        /// Only projects of this kind (picture, conveyor, device-lab, tool, research)
        #[arg(long)]
        kind: Option<String>,
        /// As JSON, for the harvest
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Discover { kind, json } => {
            let gh = discover::GitHub::new();
            let found = discover::discover(&[(&gh, &gh)], kind.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&found)?);
            } else {
                for c in &found {
                    match (&c.mark, &c.problem) {
                        (Some(m), _) => println!(
                            "{:<44} {:<11} parent {:<26} {} tables",
                            c.found.repo,
                            m.project.kind,
                            m.project.parent,
                            m.exports.len()
                        ),
                        (None, Some(p)) => println!("{:<44} NOT A MARK: {p}", c.found.repo),
                        _ => {}
                    }
                }
                println!(
                    "{} found, {} confirmed",
                    found.len(),
                    found.iter().filter(|c| c.mark.is_some()).count()
                );
            }
        }
    }
    Ok(())
}
