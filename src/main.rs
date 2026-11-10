//! oco: the Open Commons Observatory's engine.
//!
//!   oco discover [--kind KIND] [--json]   every marked project the outlets find, each confirmed by its mark
//!   oco harvest --out DIR                  their exported tables, tagged and merged, as the observatory's tad data
//!   oco site --data DIR --out DIR          the observatory's website from that data

mod discover;
mod harvest;
mod mark;
mod site;

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
    /// Read every confirmed project's exported tables into DIR: tagged, merged, canonical
    Harvest {
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Render the observatory's website from harvested data
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
        Cmd::Harvest { out } => {
            let gh = discover::GitHub::new();
            let found = discover::discover(&[(&gh, &gh)], None)?;
            let mut h = harvest::harvest(&found, &gh);
            harvest::write(&mut h, &out)?;
            println!(
                "{} projects, {} tables ({} rows), {} problems -> {}",
                h.projects.len(),
                h.tables.len(),
                h.tables.values().map(Vec::len).sum::<usize>(),
                h.problems.len(),
                out.display()
            );
        }
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
