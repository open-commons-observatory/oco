//! The mark: `oco.toml` at a repository's root - which project it is, what kind, its parent in the
//! picture, and which of its tad tables it shares (schema 1).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KINDS: [&str; 7] = [
    "observatory",
    "canon",
    "picture",
    "conveyor",
    "device-lab",
    "tool",
    "research",
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Mark {
    pub schema: u32,
    pub project: Project,
    #[serde(default)]
    pub exports: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// A short title for navigation (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Its place among its siblings in the picture (optional: lower first, then by name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    pub kind: String,
    pub parent: String,
}

impl Mark {
    /// Reads and checks a mark: schema 1, a known kind, an id of the form owner/repo.
    pub fn parse(text: &str) -> Result<Mark> {
        let m: Mark = toml::from_str(text).context("oco.toml is not valid TOML for the mark")?;
        if m.schema != 1 {
            bail!("schema {} is not one this engine reads (1)", m.schema);
        }
        if !KINDS.contains(&m.project.kind.as_str()) {
            bail!(
                "kind {:?} is not one of {}",
                m.project.kind,
                KINDS.join(", ")
            );
        }
        let parts: Vec<&str> = m.project.id.split('/').collect();
        if parts.len() != 2 || parts.iter().any(|p| p.is_empty()) {
            bail!("id {:?} is not owner/repo", m.project.id);
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCS: &str = "schema = 1\n[project]\nid = \"linux-phone-oco/docs\"\nname = \"Linux phones\"\nkind = \"picture\"\nparent = \"open-commons-observatory\"\n[exports]\nbacklog = \"data/backlog.json\"\n";

    #[test]
    fn a_mark_reads() {
        let m = Mark::parse(DOCS).unwrap();
        assert_eq!(m.project.kind, "picture");
        assert_eq!(m.exports["backlog"], "data/backlog.json");
    }

    #[test]
    fn a_wrong_mark_is_refused_with_a_reason() {
        assert!(Mark::parse(&DOCS.replace("schema = 1", "schema = 2")).is_err());
        assert!(Mark::parse(&DOCS.replace("\"picture\"", "\"blog\"")).is_err());
        assert!(Mark::parse(&DOCS.replace("linux-phone-oco/docs", "docs")).is_err());
        assert!(
            Mark::parse(&DOCS.replace("\"picture\"", "\"observatory\"")).is_ok(),
            "the observatory marks itself"
        );
    }
}
