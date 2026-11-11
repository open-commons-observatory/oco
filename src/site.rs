//! The observatory's face: a designed static website from the harvested data (tad: text as data) -
//! every word and number from the data, no JavaScript. The observatory is the meta view; each grand
//! project (a marked project of kind `picture`) has its own tab and its own views: how close it is,
//! its tables, its tests, backlog and landscape. Columns are ranked from the data, best first.

use crate::i18n::{current, t, DEFAULT, LANGUAGES};
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

type Row = Map<String, Value>;
/// A status table's cells: (row, column) -> (status, note).
type Cells = BTreeMap<(String, String), (String, String)>;

pub struct Data {
    tables: BTreeMap<String, Vec<Row>>,
}

fn s(r: &Row, k: &str) -> String {
    match r.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(v) => v.to_string(),
    }
}

fn n(r: &Row, k: &str) -> i64 {
    r.get(k)
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
}

/// Sets the text at a dotted path in a row: object keys by name, list items by their `id`.
fn set_path(row: &mut Row, path: &str, text: &str) {
    let parts: Vec<&str> = path.split('.').collect();
    let Some((last, way)) = parts.split_last() else {
        return;
    };
    if way.is_empty() {
        row.insert(last.to_string(), Value::String(text.to_string()));
        return;
    }
    let Some(mut cur) = row.get_mut(way[0]) else {
        return;
    };
    for p in &way[1..] {
        let next = match cur {
            Value::Object(m) => m.get_mut(*p),
            Value::Array(a) => a.iter_mut().find(|x| x["id"].as_str() == Some(*p)),
            _ => None,
        };
        let Some(n) = next else { return };
        cur = n;
    }
    if let Value::Object(m) = cur {
        m.insert(last.to_string(), Value::String(text.to_string()));
    }
}

/// Text made safe for HTML.
pub fn esc(x: &str) -> String {
    x.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn cut(x: &str, max: usize) -> String {
    if x.chars().count() <= max {
        x.to_string()
    } else {
        format!("{}…", x.chars().take(max).collect::<String>().trim_end())
    }
}

/// A name as a path: lowercase words joined by dashes.
pub fn slug(x: &str) -> String {
    x.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// What a status means, one of five, each with one color and one sign across the site.
pub fn tone(status: &str) -> &'static str {
    match status {
        "works" | "done" | "pass" | "yes" => "good",
        "partial" | "in-progress" => "part",
        "broken" | "missing" | "fail" | "no" => "bad",
        "not-applicable" | "n/a" => "na",
        // planned work not yet started is not failure: its own calm tone
        "open" | "planned" => "todo",
        _ => "none",
    }
}

fn sign(t: &str) -> &'static str {
    match t {
        "good" => "✓",
        "part" => "◐",
        "bad" => "✕",
        "na" => "–",
        "todo" => "○",
        _ => "?",
    }
}

fn rank(t: &str) -> u8 {
    match t {
        "good" => 4,
        "part" => 3,
        "bad" => 2,
        "none" => 1,
        _ => 0,
    }
}

impl Data {
    pub fn load(dir: &Path) -> Result<Data> {
        let mut tables = BTreeMap::new();
        for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let p = e?.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let name = p
                .file_stem()
                .and_then(|x| x.to_str())
                .unwrap_or_default()
                .to_string();
            let rows = std::fs::read_to_string(&p)?
                .lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .filter_map(|v| v.as_object().cloned())
                .collect();
            tables.insert(name, rows);
        }
        Ok(Data { tables })
    }

    /// The data as one language reads it: each project's own translations laid over its source
    /// text (ocoeco SPEC section 9). A translation applies only to rows of the project that wrote
    /// it - a project translates its own data, a lens only its own text - and what is not
    /// translated stays as written. Addresses stay the same in every language: they are fixed
    /// from the source text first.
    pub fn localized(&self, lang: &str) -> Data {
        let mut tables = self.tables.clone();
        if let Some(rows) = tables.get_mut("lenses") {
            for r in rows.iter_mut() {
                let title = r
                    .get("lens")
                    .and_then(|l| l["lens"]["title"].as_str())
                    .unwrap_or_default()
                    .to_string();
                if !title.is_empty() {
                    r.insert("slug".into(), Value::String(slug(&title)));
                }
            }
        }
        if let Some(rows) = tables.get_mut("projects") {
            for r in rows.iter_mut() {
                let name = s(r, "name");
                let title = Some(s(r, "title"))
                    .filter(|x| !x.is_empty())
                    .unwrap_or_else(|| name.split(':').next().unwrap_or(&name).trim().to_string());
                r.insert("slug".into(), Value::String(slug(&title)));
            }
        }
        // a group's key is its source category, whatever language its name is shown in
        for rows in tables.values_mut() {
            for r in rows.iter_mut() {
                if let Some(c) = r.get("category").and_then(Value::as_str).map(String::from) {
                    r.insert("category_key".into(), Value::String(c));
                }
            }
        }
        for t in self
            .rows("translations")
            .iter()
            .filter(|t| s(t, "lang") == lang)
        {
            let (table, row, project, text) =
                (s(t, "table"), s(t, "row"), s(t, "project"), s(t, "text"));
            // in the lenses table the column is the path in lens.toml (lens.title, view.features.title)
            let column = if table == "lenses" {
                format!("lens.{}", s(t, "column"))
            } else {
                s(t, "column")
            };
            let Some(rows) = tables.get_mut(&table) else {
                continue;
            };
            for r in rows
                .iter_mut()
                .filter(|r| s(r, "id") == row && s(r, "project") == project)
            {
                set_path(r, &column, &text);
            }
        }
        Data { tables }
    }

    pub fn rows(&self, t: &str) -> &[Row] {
        self.tables.get(t).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// A grand project: a marked project of kind `picture`, with its own tab.
#[derive(Debug, Clone, PartialEq)]
pub struct Grand {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub slug: String,
    pub owner: String,
    /// the family: every project of these owners, and these projects by id
    pub owners: Vec<String>,
    pub members: Vec<String>,
    /// in the observatory's navigation, at this place
    pub curated: bool,
    pub order: i64,
    /// its views: (file, title)
    pub views: Vec<(String, String)>,
    /// the lens as declared
    pub lens: Value,
}

/// The grand projects, from the registry: the title is the mark's `title`, or its name before a colon.
pub fn grands(data: &Data) -> Vec<Grand> {
    // grand views come from lenses: curated ones first, in the observatory's order
    let curated: BTreeMap<String, i64> = data
        .rows("curated")
        .iter()
        .map(|r| (s(r, "id"), n(r, "order")))
        .collect();
    let strs = |v: &Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(String::from))
            .collect()
    };
    let mut out: Vec<Grand> = data
        .rows("lenses")
        .iter()
        .map(|r| {
            let l = r.get("lens").cloned().unwrap_or(Value::Null);
            let id = s(r, "id");
            let title = l["lens"]["title"].as_str().unwrap_or(&id).to_string();
            let views = l["view"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| {
                    (
                        format!("{}.html", v["id"].as_str().unwrap_or("view")),
                        v["title"].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect();
            Grand {
                slug: Some(s(r, "slug"))
                    .filter(|x| !x.is_empty())
                    .unwrap_or_else(|| slug(&title)),
                owner: id.split('/').next().unwrap_or_default().to_string(),
                goal: l["lens"]["goal"].as_str().unwrap_or("").to_string(),
                owners: strs(&l["family"]["owners"]),
                members: strs(&l["family"]["projects"]),
                curated: curated.contains_key(&id),
                order: curated.get(&id).copied().unwrap_or(i64::MAX),
                views,
                lens: l,
                title,
                id,
            }
        })
        .collect();
    out.sort_by_key(|g| (g.order, g.title.clone()));
    out
}

/// The projects of a grand project's family: itself, and those whose parent is its owner or itself.
fn family(data: &Data, g: &Grand) -> Vec<String> {
    // as the lens declares it: every project of its owners, and the projects it names
    data.rows("projects")
        .iter()
        .map(|p| s(p, "id"))
        .filter(|id| {
            g.owners
                .iter()
                .any(|o| id.split('/').next() == Some(o.as_str()))
                || g.members.contains(id)
        })
        .collect()
}

/// One view of a lens, drawn by its kind with the parameters the lens gives.
pub fn lens_view(data: &Data, g: &Grand, fam: &[String], v: &Value, status_legend: &str) -> String {
    let k = |key: &str| v[key].as_str().unwrap_or("").to_string();
    let lead = k("lead");
    let head = format!(
        r#"<h1>{}</h1>{}"#,
        esc(&k("title")),
        if lead.is_empty() {
            String::new()
        } else {
            format!(r#"<p class="lead">{}</p>"#, esc(&lead))
        }
    );
    let label = Some(k("row_title"))
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| k("rows"));
    match k("kind").as_str() {
        "path" => how_close(data, g, fam),
        "matrix" => {
            let rows = of(data, &k("table"), fam);
            let cells = cells_of(&rows, &k("row_key"), &k("column_key"));
            let mut lines = lines_from(
                data,
                &k("rows"),
                |id| cells.keys().any(|(r, _)| r == id),
                "why",
            );
            let order: Vec<&str> = v["groups"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if !order.is_empty() {
                let key: BTreeMap<String, String> = data
                    .rows(&k("rows"))
                    .iter()
                    .map(|r| (s(r, "id"), first(r, &["category_key", "category"])))
                    .collect();
                lines.sort_by_key(|l| {
                    order
                        .iter()
                        .position(|g| key.get(&l.id).is_some_and(|c| c == g))
                        .unwrap_or(usize::MAX)
                });
            }
            let ids: Vec<String> = lines.iter().map(|l| l.id.clone()).collect();
            let cols = ranked(
                &cells,
                &ids,
                data.rows(&k("columns"))
                    .iter()
                    .map(|c| (s(c, "id"), s(c, "name"), s(c, "kind")))
                    .filter(|(id, _, _)| cells.keys().any(|(_, c)| c == id))
                    .collect(),
            );
            format!(
                "{head}{status_legend}{}",
                table(&label, &lines, &cols, &cells)
            )
        }
        "families" => {
            let support = of(data, &k("table"), fam);
            let fcells = families(&support, data.rows(&k("columns")));
            let lines = lines_from(
                data,
                &k("rows"),
                |id| fcells.keys().any(|(r, _)| r == id),
                "why",
            );
            let ids: Vec<String> = lines.iter().map(|l| l.id.clone()).collect();
            let mut groups: Vec<(String, String, String)> = fcells
                .keys()
                .map(|(_, f)| (f.clone(), f.clone(), String::new()))
                .collect();
            groups.sort();
            groups.dedup();
            format!(
                "{head}{status_legend}{}",
                table(&label, &lines, &ranked(&fcells, &ids, groups), &fcells)
            )
        }
        "runs" => format!(
            r#"{head}{}<div class="scroll" style="padding:16px">{}</div>"#,
            legend(&[
                ("good", "⟦passed⟧"),
                ("bad", "⟦failed⟧"),
                ("none", "⟦skipped⟧")
            ]),
            runs_chart(&of(data, "test_runs", fam))
        ),
        "backlog" => backlog(&of(data, "backlog", fam)),
        "cards" => {
            let mut b = format!(r#"{head}<div class="cards" style="margin-top:28px">"#);
            for r in of(data, &k("table"), fam) {
                let note = s(r, &k("note_field"));
                let _ = write!(
                    b,
                    r#"<div class="card"><h4><a href="{}">{}</a></h4><p>{}</p>{}</div>"#,
                    esc(&s(r, &k("link_field"))),
                    esc(&s(r, &k("title_field"))),
                    esc(&s(r, &k("text_field"))),
                    if note.is_empty() {
                        String::new()
                    } else {
                        format!(r#"<p><b>For us:</b> {}</p>"#, esc(&note))
                    }
                );
            }
            b + "</div>"
        }
        "heartbeat" => format!("{head}{}", made_by_work(data, fam, now_secs())),
        "vocabulary" => vocabulary_html(data, fam),
        "handwork" => handwork_html(data, fam),
        // the signature view: the whole chain of the lens's subject on one page
        "essence" => {
            let subject = Some(k("subject"))
                .filter(|x| !x.is_empty())
                .unwrap_or_else(|| g.slug.clone());
            let ms: Vec<Milestone> = milestones_of(&of(data, "milestones", fam))
                .into_iter()
                .filter(|m| m.subject == subject)
                .collect();
            // the lens's own name on its landing; another essence inside it carries its view's name
            let own = v.get("id").is_some_and(|i| i == "index");
            let (title, goal) = if own {
                (g.title.clone(), g.goal.clone())
            } else {
                (k("title"), k("lead"))
            };
            essence_html(data, fam, &k("project"), &ms, &title, &goal)
        }
        // the idea of the project the lens is about - or of each project of its family that wrote one
        "idea" => {
            let only = k("project");
            let projects: Vec<String> = if only.is_empty() {
                fam.to_vec()
            } else {
                vec![only]
            };
            let written: Vec<&String> = projects
                .iter()
                .filter(|p| data.rows("idea").iter().any(|r| s(r, "project") == **p))
                .collect();
            let mut b = String::new();
            // one project: its name leads the page, for a reader arriving with nothing
            if written.len() == 1 {
                let _ = write!(b, "<h1>{}</h1>", esc(&g.title));
            }
            for p in &written {
                if written.len() > 1 {
                    let name = data
                        .rows("projects")
                        .iter()
                        .find(|x| s(x, "id") == **p)
                        .map(|x| first(x, &["title", "name"]))
                        .unwrap_or_else(|| (*p).clone());
                    let _ = write!(b, "<h1>{}</h1>", esc(&name));
                }
                b.push_str(&mark_words(
                    &idea_html(&of(data, "idea", std::slice::from_ref(*p))),
                    &own_words(data, std::slice::from_ref(*p)),
                ));
            }
            if b.is_empty() {
                b = idea_html(&[]);
            }
            b
        }
        "ideas" => {
            let mut b = format!(r#"{head}<div class="cards" style="margin-top:28px">"#);
            for r in of(data, "concepts", fam) {
                let why = s(r, "why");
                let _ = write!(
                    b,
                    r#"<div class="card"><h4>{}</h4><p>{}</p>{}</div>"#,
                    esc(&s(r, "name")),
                    esc(&first(r, &["summary", "statement", "description"])),
                    if why.is_empty() {
                        String::new()
                    } else {
                        format!(r#"<p><b>⟦why⟧</b> {}</p>"#, esc(&why))
                    }
                );
            }
            for r in of(data, "decisions", fam) {
                let _ = write!(
                    b,
                    r#"<div class="card"><h4>{}</h4><p>{}</p><p><b>⟦rejected⟧</b> {}</p><p><span class="chip">{}</span></p></div>"#,
                    esc(&s(r, "decision")),
                    esc(&s(r, "why")),
                    esc(&s(r, "rejected")),
                    esc(&s(r, "date"))
                );
            }
            b + "</div>"
        }
        "log" => {
            let mut logs = of(data, "session_log", fam);
            logs.sort_by_key(|r| std::cmp::Reverse(s(r, "entry_date")));
            let mut b = head.clone();
            for r in logs {
                let rejected = if s(r, "rejected").is_empty() {
                    String::new()
                } else {
                    format!(r#"<p><b>⟦rejected⟧</b> {}</p>"#, esc(&s(r, "rejected")))
                };
                let _ = write!(
                    b,
                    r#"<div class="log"><time>{} · {}</time><h4>{}</h4><p>{}</p>{rejected}</div>"#,
                    esc(&s(r, "entry_date")),
                    esc(&from(r)),
                    esc(&s(r, "title")),
                    esc(&s(r, "done"))
                );
            }
            b
        }
        other => format!(
            r#"{head}<p class="lead">The view kind "{}" is not drawn yet.</p>"#,
            esc(other)
        ),
    }
}

fn of<'a>(data: &'a Data, table: &str, fam: &[String]) -> Vec<&'a Row> {
    data.rows(table)
        .iter()
        .filter(|r| fam.contains(&s(r, "project")))
        .collect()
}

/// A column of a status table: id, name, the share of its rows that work (0..1).
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub id: String,
    pub name: String,
    pub share: f64,
    pub kind: String,
}

/// Columns ranked from the cells: the most working first (ties by name).
pub fn ranked(
    cells: &Cells,
    row_ids: &[String],
    cols: Vec<(String, String, String)>,
) -> Vec<Column> {
    let mut out: Vec<Column> = cols
        .into_iter()
        .map(|(id, name, kind)| {
            let works = row_ids
                .iter()
                .filter(|r| {
                    cells
                        .get(&((*r).clone(), id.clone()))
                        .is_some_and(|(st, _)| tone(st) == "good")
                })
                .count();
            let share = if row_ids.is_empty() {
                0.0
            } else {
                works as f64 / row_ids.len() as f64
            };
            Column {
                id,
                name,
                share,
                kind,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.share
            .partial_cmp(&a.share)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// A row of a status table: id, name, its group, and a note on what it is.
pub struct Line {
    pub id: String,
    pub name: String,
    pub group: String,
    pub about: String,
}

/// A status table: rows under group rows, ranked columns with their share, a sign in every cell.
pub fn table(first: &str, lines: &[Line], cols: &[Column], cells: &Cells) -> String {
    let mut t = format!(
        r#"<div class="scroll"><table class="grid"><thead><tr><th class="first">{}</th>"#,
        esc(first)
    );
    for c in cols {
        let _ = write!(
            t,
            r#"<th><span>{}</span><small>{:.0}%</small></th>"#,
            esc(&c.name),
            c.share * 100.0
        );
    }
    t.push_str("</tr></thead><tbody>");
    let mut group = None::<&str>;
    for l in lines {
        if group != Some(l.group.as_str()) {
            group = Some(l.group.as_str());
            let _ = write!(
                t,
                r#"<tr class="group"><th colspan="{}">{}</th></tr>"#,
                cols.len() + 1,
                esc(if l.group.is_empty() {
                    "other"
                } else {
                    &l.group
                })
            );
        }
        let _ = write!(
            t,
            r#"<tr><th title="{}">{}</th>"#,
            esc(&l.about),
            esc(&l.name)
        );
        for c in cols {
            let (st, note) = cells
                .get(&(l.id.clone(), c.id.clone()))
                .cloned()
                .unwrap_or_else(|| ("unknown".into(), String::new()));
            let tn = tone(&st);
            let tip = if note.is_empty() {
                format!("{} on {}: {st}", l.name, c.name)
            } else {
                format!("{} on {}: {st} - {note}", l.name, c.name)
            };
            let _ = write!(
                t,
                r#"<td class="s {tn}" title="{}">{}</td>"#,
                esc(&cut(&tip, 320)),
                sign(tn)
            );
        }
        t.push_str("</tr>");
    }
    t + "</tbody></table></div>"
}

fn lines_from(
    data: &Data,
    reference: &str,
    used: impl Fn(&str) -> bool,
    about_key: &str,
) -> Vec<Line> {
    let mut groups: Vec<String> = Vec::new();
    let mut all: Vec<Line> = Vec::new();
    for r in data.rows(reference) {
        let id = s(r, "id");
        if !used(&id) {
            continue;
        }
        let group = s(r, "category");
        if !groups.contains(&group) {
            groups.push(group.clone());
        }
        all.push(Line {
            name: Some(s(r, "name"))
                .filter(|x| !x.is_empty())
                .unwrap_or_else(|| id.clone()),
            id,
            group,
            about: s(r, about_key),
        });
    }
    all.sort_by_key(|l| {
        groups
            .iter()
            .position(|g| *g == l.group)
            .unwrap_or(usize::MAX)
    });
    all
}

fn cells_of(rows: &[&Row], row_key: &str, col_key: &str) -> Cells {
    rows.iter()
        .map(|r| {
            (
                (s(r, row_key), s(r, col_key)),
                (
                    s(r, "status"),
                    Some(s(r, "note"))
                        .filter(|x| !x.is_empty())
                        .unwrap_or_else(|| s(r, "notes")),
                ),
            )
        })
        .collect()
}

/// Support by SoC family: each family's best status across its devices, per component.
pub fn families(support: &[&Row], devices: &[Row]) -> Cells {
    let soc: BTreeMap<String, String> = devices.iter().map(|d| (s(d, "id"), s(d, "soc"))).collect();
    let mut best: Cells = BTreeMap::new();
    for r in support {
        let fam = soc
            .get(&s(r, "device_id"))
            .cloned()
            .unwrap_or_else(|| "unknown SoC".into());
        let key = (s(r, "component_id"), fam);
        let st = s(r, "status");
        let dev = s(r, "device_id");
        let better = best
            .get(&key)
            .is_none_or(|(cur, _)| rank(tone(&st)) > rank(tone(cur)));
        if better {
            best.insert(key, (st, format!("best on {dev}")));
        }
    }
    best
}

const STYLE: &str = r#":root{--bg:#fbfaf8;--fg:#1d1d1f;--mute:#6e6e73;--line:#e6e3de;--card:#fff;--accent:#3b5bdb;--soft:#f3f1ed;
--good:#2f9e6b;--part:#d9a43b;--bad:#d1495b;--none:#c9ccd1;--na:#ecedef}
@media(prefers-color-scheme:dark){:root{--bg:#141416;--fg:#ececf0;--mute:#9a9aa3;--line:#2a2a2f;--card:#1c1c20;--accent:#8ea2ff;--soft:#1f1f24;--none:#4a4c52;--na:#2a2b30}}
*{box-sizing:border-box}html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.6 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
a{color:var(--accent);text-decoration:none}a:hover{text-decoration:underline}
.wrap{max-width:1180px;margin:0 auto;padding:0 24px}
header{border-bottom:1px solid var(--line);background:var(--bg);position:sticky;top:0;z-index:3}
header .wrap{display:flex;align-items:center;gap:28px;height:60px;overflow-x:auto}
.mark{font-weight:650;letter-spacing:-.01em;white-space:nowrap;color:var(--fg)}
nav{display:flex;gap:22px;font-size:15px}nav a{color:var(--mute);white-space:nowrap}nav a.on{color:var(--fg);font-weight:600}
.sub{border-bottom:1px solid var(--line);background:var(--soft)}.sub .wrap{display:flex;gap:20px;height:46px;align-items:center;overflow-x:auto;font-size:14.5px}
.sub a{color:var(--mute);white-space:nowrap}.sub a.on{color:var(--fg);font-weight:600}.sub b{margin-right:8px;white-space:nowrap}
main{padding:44px 0 72px}h1{font-size:40px;line-height:1.15;letter-spacing:-.02em;margin:0 0 12px;font-weight:700}
h2{font-size:22px;letter-spacing:-.01em;margin:48px 0 16px}h3{font-size:14px;margin:28px 0 10px;color:var(--mute);text-transform:uppercase;letter-spacing:.06em}
.lead{font-size:19px;color:var(--mute);max-width:760px;margin:0}
.stats{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:16px;margin-top:32px}
.stat{background:var(--card);border:1px solid var(--line);border-radius:14px;padding:18px 20px}
.stat b{display:block;font-size:30px;letter-spacing:-.02em}.stat span{color:var(--mute);font-size:14px}
.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:16px}
.card{background:var(--card);border:1px solid var(--line);border-radius:14px;padding:18px 20px}
.card h4{margin:0 0 6px;font-size:17px}.card p{margin:6px 0 0;color:var(--mute);font-size:14.5px}
.grand{display:block;background:var(--card);border:1px solid var(--line);border-radius:18px;padding:26px 28px;margin-top:20px;color:var(--fg)}
.grand:hover{text-decoration:none;border-color:var(--accent)}.grand h3{margin:0;color:var(--fg);font-size:24px;text-transform:none;letter-spacing:-.01em}
.grand p{margin:6px 0 0;color:var(--mute)}
.chip{display:inline-block;font-size:12px;padding:2px 9px;border-radius:99px;border:1px solid var(--line);color:var(--mute);margin-right:6px}
.log{border-left:2px solid var(--line);padding:2px 0 2px 18px;margin:18px 0}.log time{color:var(--mute);font-size:13px}
.log h4{margin:2px 0 4px;font-size:16px}.log p{margin:0;color:var(--mute);font-size:14.5px}
.scroll{overflow-x:auto;border:1px solid var(--line);border-radius:14px;background:var(--card);margin-top:16px}
table.grid{border-collapse:collapse;width:100%;font-size:14.5px}
.grid th,.grid td{border-bottom:1px solid var(--line);padding:9px 12px}
.grid thead th{position:sticky;top:0;background:var(--card);font-weight:600;text-align:center;vertical-align:bottom;border-bottom:2px solid var(--line)}
.grid thead th span{display:block;line-height:1.25}.grid thead th small{display:block;color:var(--mute);font-weight:500;margin-top:2px}
.grid thead th.first{text-align:left}.grid tbody th{text-align:left;font-weight:500;white-space:nowrap}
.grid tr.group th{background:var(--soft);color:var(--mute);font-size:12.5px;text-transform:uppercase;letter-spacing:.06em;font-weight:600;padding:7px 12px}
.grid td.s{text-align:center;font-weight:700;width:1%;white-space:nowrap;color:#fff}
.s.good{background:color-mix(in srgb,var(--good) 85%,transparent)}.s.part{background:color-mix(in srgb,var(--part) 85%,transparent)}
.s.bad{background:color-mix(in srgb,var(--bad) 85%,transparent)}.grid td.s.none{background:var(--soft);color:var(--mute)}.grid td.s.na{background:var(--na);color:var(--mute)}
.legend{display:flex;gap:18px;flex-wrap:wrap;color:var(--mute);font-size:14px;margin:18px 0 0}.legend i{display:inline-block;width:20px;height:20px;line-height:20px;text-align:center;border-radius:5px;margin-right:6px;font-style:normal;font-weight:700;color:#fff;font-size:12px}
.legend .none,.legend .na{color:var(--mute)}.legend .good{background:var(--good)}.legend .part{background:var(--part)}.legend .bad{background:var(--bad)}.legend .none{background:var(--soft)}.legend .na{background:var(--na)}
.bars{margin-top:24px}.bar{display:grid;grid-template-columns:220px 1fr 56px;gap:14px;align-items:center;margin:10px 0}
.bar span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.stack{display:flex;height:14px;border-radius:99px;overflow:hidden;background:var(--soft)}.stack i{display:block;height:100%}
.stack .good{background:var(--good)}.stack .part{background:var(--part)}.stack .bad{background:var(--bad)}.stack .none{background:var(--none)}.bar b{text-align:right;font-weight:600}
.board{display:grid;grid-template-columns:repeat(3,1fr);gap:20px}@media(max-width:820px){.board{grid-template-columns:1fr}.bar{grid-template-columns:120px 1fr 48px}}
.col .card{margin-bottom:12px}
.pathbox{background:var(--card);border:1px solid var(--line);border-radius:18px;padding:14px 10px}
.signs{display:flex;flex-wrap:wrap;gap:3px}.mini{display:inline-block;width:18px;height:18px;line-height:18px;text-align:center;border-radius:4px;font-size:11px;font-weight:700;color:#fff}
.mini.good{background:var(--good)}.mini.part{background:var(--part)}.mini.bad{background:var(--bad)}.mini.none{background:var(--soft);color:var(--mute)}.mini.na{background:var(--na);color:var(--mute)}
.psub{display:flex;gap:20px;align-items:center;flex-wrap:wrap;font-size:14.5px;padding:0 0 16px;margin-bottom:28px;border-bottom:1px solid var(--line)}
.psub a{color:var(--mute)}.psub a.on{color:var(--fg);font-weight:600}.psub b{margin-right:6px}
svg.spark{display:block;margin:6px 0 2px}svg.spark .busy{fill:var(--good)}svg.spark .idle{fill:var(--line)}
.work{margin:6px 8px;display:flex;flex-wrap:wrap;gap:8px;align-items:center}.work b{font-size:22px}
.chip.ci.good{color:var(--good);border-color:var(--good)}.chip.ci.bad{color:var(--bad);border-color:var(--bad)}.chip.ci.part{color:var(--part);border-color:var(--part)}
.langs{margin-left:auto;display:flex;align-items:center;gap:7px;font-size:14px;white-space:nowrap;color:var(--mute)}
.langs a{color:var(--mute)}.langs b{color:var(--fg);font-weight:600}.langs .dot{opacity:.6}
.path .bands{display:flex;gap:8px;overflow-x:auto}.path .band{background:var(--soft);border-radius:14px;padding:10px 6px 12px}
.path .hlabel{font-size:13px;color:var(--mute);font-weight:600;letter-spacing:.04em}.path .band .hlabel{padding:0 8px 10px}
.path .stops{display:flex}.stopcell{position:relative;flex:1 1 0;min-width:84px;display:flex;flex-direction:column;align-items:center;padding:0 5px}
.stopcell::before{content:"";position:absolute;top:17px;left:0;right:0;height:6px;background:var(--line)}.stopcell.passed::before{background:var(--good)}
.stopcell svg{position:relative;z-index:1}.path .stop{fill:var(--card);stroke:var(--line);stroke-width:5}.path .ring{fill:none;stroke-width:5;stroke-linecap:round}
.path .ring.good{stroke:var(--good)}.path .ring.part{stroke:var(--part)}.path .ring.bad{stroke:var(--bad)}.path .ring.none{stroke:var(--none)}
.frac{position:absolute;top:13px;left:0;right:0;z-index:2;text-align:center;font-size:10.5px;line-height:14px;font-weight:700}
.stitle{margin-top:7px;max-width:100%;text-align:center;font-size:12.5px;line-height:1.25;overflow-wrap:anywhere;hyphens:auto}
.ladder{display:flex;flex-wrap:wrap;align-items:center;gap:10px;padding:12px 8px 2px}.steps{display:inline-flex;gap:4px}
.lvl{display:inline-block;width:26px;height:10px;border-radius:3px;background:var(--soft);border:1px solid var(--line)}.lvl.on{background:var(--accent);border-color:var(--accent)}
.path .ring.todo{stroke:var(--none)}.mini.todo{background:transparent;border:1px solid var(--none);color:var(--mute);line-height:16px}
.idea{max-width:760px}.idea p{font-size:18px;line-height:1.7;margin:0 0 18px}.idea .first p{font-size:23px;line-height:1.5;letter-spacing:-.01em;margin-bottom:26px}
.idea h2{font-size:26px;margin:44px 0 14px}.idea blockquote{margin:26px 0;padding:4px 0 4px 20px;border-left:3px solid var(--accent)}.idea blockquote p{color:var(--mute);margin-bottom:10px}
.idea blockquote b{display:block;color:var(--fg);margin-bottom:8px;font-size:18px}.idea .limit{background:var(--soft);border-radius:14px;padding:18px 22px 6px;margin:32px 0}.idea .limit h3{margin-top:0}
.told{display:flex;gap:10px;flex-wrap:wrap;margin:22px 0 36px}.tell{font-size:13.5px;padding:4px 12px;border-radius:99px;border:1px solid var(--line);color:var(--mute)}.tell.on{border-color:var(--good);color:var(--good)}
.flow{border-left:3px solid var(--line);margin-left:18px;padding-left:36px}.step{position:relative;margin-bottom:48px}
.step::before{content:attr(data-n);position:absolute;left:-57px;top:-6px;width:38px;height:38px;border-radius:50%;background:var(--accent);color:var(--bg);display:flex;align-items:center;justify-content:center;font-weight:700}
.step>h3{margin:0 0 14px}.opening p{font-size:21px;line-height:1.5;max-width:780px;margin:0}.mute{color:var(--mute)}
.serve{display:grid;grid-template-columns:minmax(180px,250px) 1fr;gap:18px;padding:16px 0;border-top:1px solid var(--line)}.serve .what b{display:block;margin-bottom:8px}
.by{display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:12px}.working{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:12px 14px}
.working b{display:block;margin-bottom:6px}.working p{margin:8px 0 0;font-size:14.5px}.working .why{color:var(--mute)}
@media(max-width:820px){.serve{grid-template-columns:1fr}.flow{margin-left:8px;padding-left:28px}.step::before{left:-49px}}
.tells{display:flex;gap:10px;flex-wrap:wrap;margin:10px 0 0;font-size:12.5px}.tells .on{color:var(--good)}.tells .off{color:var(--mute)}
abbr.word{text-decoration:underline dotted;text-underline-offset:3px;cursor:help}dfn{font-style:normal}
.attention{background:var(--soft);border-radius:16px;padding:18px 22px 8px;margin:26px 0 34px}.attention h2{margin-top:0}.attn{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:18px}
.attn h3{font-size:15px;margin:0 0 8px}.attn ul{margin:0;padding-left:18px}.attn li{margin:0 0 7px;font-size:14.5px}
.hand .hrow{display:grid;grid-template-columns:minmax(200px,1fr) minmax(80px,220px) 56px auto;gap:14px;align-items:center;padding:9px 0;border-top:1px solid var(--line)}
.hbarbox{display:block;background:var(--soft);border-radius:4px;height:8px}.hbar{display:block;height:8px;border-radius:4px;background:var(--mute)}.hbar.manual{background:var(--accent)}.hbar.automated{background:var(--good)}
.hrow.sat .hact{font-weight:600}.chip.warn{border-color:var(--accent);color:var(--accent)}@media(max-width:640px){.hand .hrow{grid-template-columns:1fr 56px}.hand .hbarbox,.hand .hrow .chip:last-child{display:none}}
footer{border-top:1px solid var(--line);color:var(--mute);font-size:14px;padding:28px 0}
"#;

struct Page<'a> {
    grands: &'a [Grand],
    generated: &'a str,
}

impl Page<'_> {
    /// A page: the observatory's tabs (itself, then each grand project), a grand project's own views.
    fn render(&self, grand: Option<&Grand>, view: &str, title: &str, body: &str) -> String {
        localize(&self.render_raw(grand, view, title, body))
    }

    fn render_raw(&self, grand: Option<&Grand>, view: &str, title: &str, body: &str) -> String {
        let up = if grand.is_some() { "../" } else { "" };
        let here = |v: &str| {
            if grand.is_none() && view == v {
                r#" class="on""#
            } else {
                ""
            }
        };
        let mut nav = format!(
            r#"<a href="{up}index.html"{}>{}</a><a href="{up}how-we-work.html"{}>{}</a>"#,
            here(""),
            esc(&t("nav-observatory")),
            here("how-we-work.html"),
            esc(&t("nav-how-we-work"))
        );
        for g in self.grands.iter().filter(|g| g.curated) {
            let on = grand.is_some_and(|x| x.id == g.id);
            let _ = write!(
                nav,
                r#"<a href="{up}{}/index.html"{}>{}</a>"#,
                g.slug,
                if on { r#" class="on""# } else { "" },
                esc(&g.title)
            );
        }
        let sub = grand
            .map(|g| {
                let mut x = format!(
                    r#"<div class="sub"><div class="wrap"><b>{}</b>"#,
                    esc(&g.title)
                );
                for (f, label) in &g.views {
                    let _ = write!(
                        x,
                        r#"<a href="{f}"{}>{}</a>"#,
                        if f == view { r#" class="on""# } else { "" },
                        esc(label)
                    );
                }
                x + "</div></div>"
            })
            .unwrap_or_default();
        // the same page in every language: its path below the language's folder
        let path = match grand {
            Some(g) => format!("{}/{}", g.slug, view),
            None if view.is_empty() => "index.html".to_string(),
            None => view.to_string(),
        };
        let lang = current();
        let (mut langs, mut alternates) = (String::new(), String::new());
        for l in LANGUAGES.iter() {
            let href = format!("{up}../{}/{path}", l.code);
            let _ = write!(
                alternates,
                r#"<link rel="alternate" hreflang="{}" href="{href}">"#,
                l.code
            );
            if !langs.is_empty() {
                langs.push_str(r#"<span class="dot">·</span>"#);
            }
            if l.code == lang {
                let _ = write!(langs, r#"<b lang="{0}">{1}</b>"#, l.code, l.name);
            } else {
                let _ = write!(
                    langs,
                    r#"<a href="{href}" hreflang="{0}" lang="{0}">{1}</a>"#,
                    l.code, l.name
                );
            }
        }
        format!(
            r#"<!doctype html><html lang="{lang}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{t} · Open Commons Observatory</title><link rel="stylesheet" href="{up}style.css">{alternates}</head><body>
<header><div class="wrap"><a class="mark" href="{up}index.html">Open Commons Observatory</a><nav>{nav}</nav><span class="langs" aria-label="{langs_label}">{langs}</span></div></header>{sub}
<main><div class="wrap">{body}</div></main>
<footer><div class="wrap">{footer} · {g}</div></footer>
</body></html>
"#,
            t = esc(title),
            g = esc(self.generated),
            langs_label = esc(&t("languages")),
            footer = esc(&t("footer"))
        )
    }
}

fn legend(items: &[(&str, &str)]) -> String {
    let mut l = String::from(r#"<div class="legend">"#);
    for (t, label) in items {
        let _ = write!(l, r#"<span><i class="{t}">{}</i>{label}</span>"#, sign(t));
    }
    l + "</div>"
}

fn feature_view(data: &Data, fam: &[String]) -> (Vec<Line>, Vec<Column>, Cells) {
    let support = of(data, "feature_support", fam);
    let cells = cells_of(&support, "feature_id", "platform_id");
    let lines = lines_from(
        data,
        "features",
        |id| cells.keys().any(|(r, _)| r == id),
        "why",
    );
    let ids: Vec<String> = lines.iter().map(|l| l.id.clone()).collect();
    let cols: Vec<(String, String, String)> = data
        .rows("platforms")
        .iter()
        .filter(|p| cells.keys().any(|(_, c)| *c == s(p, "id")))
        .map(|p| (s(p, "id"), s(p, "name"), s(p, "kind")))
        .collect();
    (lines, ranked(&cells, &ids, cols), cells)
}

/// How a candidate stands on the measured rows: works, partly, missing or broken, not known yet.
pub fn tally(cells: &Cells, ids: &[String], col: &str) -> [usize; 4] {
    let mut t = [0; 4];
    for r in ids {
        let st = cells
            .get(&(r.clone(), col.to_string()))
            .map(|(x, _)| x.as_str())
            .unwrap_or("unknown");
        match tone(st) {
            "good" => t[0] += 1,
            "part" => t[1] += 1,
            "bad" => t[2] += 1,
            "na" => {}
            _ => t[3] += 1,
        }
    }
    t
}

fn how_close(data: &Data, g: &Grand, fam: &[String]) -> String {
    let (lines, cols, cells) = feature_view(data, fam);
    let ids: Vec<String> = lines.iter().map(|l| l.id.clone()).collect();
    let mut b = format!(
        r#"<h1>{}</h1><p class="lead">{}</p>"#,
        esc(&g.title),
        esc(if g.goal.is_empty() {
            "How close it is."
        } else {
            &g.goal
        })
    );
    let ms = milestones_of(&of(data, "milestones", fam));
    let mine: Vec<Milestone> = ms.iter().filter(|m| m.subject == g.slug).cloned().collect();
    if !mine.is_empty() {
        b.push_str(&path_section(data, "⟦the-path⟧", &mine));
        b.push_str(&made_by_work(data, fam, now_secs()));
    }
    if cols.is_empty() {
        return b;
    }
    if let Some(l) = cols.iter().find(|c| c.kind == "linux") {
        let [w, p, m, u] = tally(&cells, &ids, &l.id);
        let _ = write!(
            b,
            r#"<div class="stats"><div class="stat"><b>{w}</b><span>⟦features-known-to-work-on⟧ {}</span></div><div class="stat"><b>{p}</b><span>⟦partly⟧</span></div><div class="stat"><b>{m}</b><span>⟦missing⟧</span></div><div class="stat"><b>{u}</b><span>⟦not-known-to-measure⟧</span></div></div>"#,
            esc(&l.name)
        );
    }
    b.push_str(r#"<h2>⟦every-candidate⟧</h2>"#);
    b.push_str(&legend(&[
        ("good", "⟦works⟧"),
        ("part", "⟦partly⟧"),
        ("bad", "⟦missing⟧"),
        ("none", "⟦not-known-yet⟧"),
    ]));
    b.push_str(r#"<div class="bars">"#);
    for c in &cols {
        let t = tally(&cells, &ids, &c.id);
        let total = t.iter().sum::<usize>().max(1) as f64;
        let _ = write!(
            b,
            r#"<div class="bar"><span>{}</span><div class="stack">"#,
            esc(&c.name)
        );
        for (v, cls, what) in [
            (t[0], "good", "work"),
            (t[1], "part", "partly"),
            (t[2], "bad", "missing"),
            (t[3], "none", "not known yet"),
        ] {
            if v > 0 {
                let _ = write!(
                    b,
                    r#"<i class="{cls}" style="width:{:.2}%" title="{v} {what}"></i>"#,
                    v as f64 / total * 100.0
                );
            }
        }
        let _ = write!(b, r#"</div><b>{}/{}</b></div>"#, t[0], ids.len());
    }
    b.push_str("</div><h2>⟦by-area⟧</h2>");
    let mut groups: Vec<&str> = Vec::new();
    for l in &lines {
        if !groups.contains(&l.group.as_str()) {
            groups.push(&l.group);
        }
    }
    let mut area_cells = BTreeMap::new();
    let mut area_lines = Vec::new();
    for gname in groups {
        let in_g: Vec<String> = lines
            .iter()
            .filter(|l| l.group == gname)
            .map(|l| l.id.clone())
            .collect();
        for c in &cols {
            let [w, p, m, u] = tally(&cells, &in_g, &c.id);
            let st = if u == in_g.len() {
                "unknown"
            } else if w == in_g.len() {
                "works"
            } else if w + p > 0 {
                "partial"
            } else if m > 0 {
                "missing"
            } else {
                "unknown"
            };
            area_cells.insert(
                (gname.to_string(), c.id.clone()),
                (
                    st.to_string(),
                    format!("{w} work, {p} partly, {m} missing, {u} not known yet"),
                ),
            );
        }
        area_lines.push(Line {
            id: gname.to_string(),
            name: gname.to_string(),
            group: String::from("areas"),
            about: format!("{} features", in_g.len()),
        });
    }
    b.push_str(&table("Area", &area_lines, &cols, &area_cells));
    let names: BTreeMap<String, String> = data
        .rows("devices")
        .iter()
        .map(|d| (s(d, "id"), s(d, "name")))
        .collect();
    let mut subjects: Vec<String> = ms
        .iter()
        .map(|m| m.subject.clone())
        .filter(|x| *x != g.slug)
        .collect();
    subjects.dedup();
    for subj in subjects {
        let theirs: Vec<Milestone> = ms.iter().filter(|m| m.subject == subj).cloned().collect();
        b.push_str(&path_section(
            data,
            &format!(
                "The path of {}",
                names.get(&subj).cloned().unwrap_or(subj.clone())
            ),
            &theirs,
        ));
    }
    b
}

fn grand_overview_card(data: &Data, g: &Grand) -> String {
    let fam = family(data, g);
    let (lines, cols, cells) = feature_view(data, &fam);
    let ids: Vec<String> = lines.iter().map(|l| l.id.clone()).collect();
    let line = match cols.iter().find(|c| c.kind == "linux") {
        Some(l) => {
            let [w, p, m, u] = tally(&cells, &ids, &l.id);
            format!("{w} / {} ⟦card-features-work⟧ {}, {p} ⟦card-partly⟧, {m} ⟦card-missing⟧, {u} ⟦card-to-measure⟧ · ", ids.len(), l.name)
        }
        None => String::new(),
    };
    let ms: Vec<Milestone> = milestones_of(&of(data, "milestones", &fam))
        .into_iter()
        .filter(|m| m.subject == g.slug)
        .collect();
    let rated: Vec<(Milestone, Evidence)> = ms
        .iter()
        .map(|m| {
            let items: Vec<(String, String)> = m
                .criteria
                .iter()
                .flat_map(|c| evaluate(data, &m.project, c))
                .collect();
            (m.clone(), evidence(&items))
        })
        .collect();
    let path = if rated.is_empty() {
        String::new()
    } else {
        path_view(&rated)
    };
    format!(
        r#"<a class="grand" href="{}/index.html"><h3>{}</h3><p>{}</p>{path}<p>{}{}</p></a>"#,
        g.slug,
        esc(&g.title),
        esc(&g.goal),
        esc(&line),
        esc(&crate::i18n::tn("card-projects", fam.len() as i64))
    )
}

fn observatory(data: &Data, grands: &[Grand]) -> String {
    let projects = data.rows("projects");
    let open = data
        .rows("backlog")
        .iter()
        .filter(|r| s(r, "status") != "done")
        .count();
    let attention = attention_html(data);
    let mut b = format!(
        r#"<h1>Open Commons Observatory</h1><p class="lead">⟦observatory-lead⟧</p>
<div class="stats"><div class="stat"><b>{}</b><span>⟦stat-grand-projects⟧</span></div><div class="stat"><b>{}</b><span>⟦stat-projects⟧</span></div><div class="stat"><b>{open}</b><span>⟦stat-open-items⟧</span></div><div class="stat"><b>{}</b><span>⟦stat-log-entries⟧</span></div></div>{attention}<h2>⟦grand-projects⟧</h2>"#,
        grands.len(),
        projects.len(),
        data.rows("session_log").len()
    );
    for g in grands.iter().filter(|g| g.curated) {
        b.push_str(&grand_overview_card(data, g));
    }
    if grands.iter().any(|g| !g.curated) {
        b.push_str(r#"<h2>⟦discovered-lenses⟧</h2><p class="lead">⟦discovered-lenses-lead⟧</p>"#);
        for g in grands.iter().filter(|g| !g.curated) {
            b.push_str(&grand_overview_card(data, g));
        }
    }
    // the other projects with a path of their own: a card each, linking to its pages
    let all = subs(data);
    for (heading, home) in [("⟦observatory-itself⟧", true), ("⟦other-projects⟧", false)] {
        let group: Vec<&Sub> = all
            .iter()
            .filter(|p| p.id.starts_with(HOME) == home)
            .collect();
        if group.is_empty() {
            continue;
        }
        let _ = write!(b, "<h2>{heading}</h2>");
        for p in group {
            let ms = milestones_of(&of(data, "milestones", std::slice::from_ref(&p.id)));
            let rated = rate(data, &ms);
            let done = rated.iter().filter(|(_, e)| e.state() == "done").count();
            let _ = write!(
                b,
                r#"<a class="grand" href="{}.html"><h3>{}</h3><p>{}</p>{}<p>{done} / {} ⟦milestones-done⟧ · ⟦card-readiness⟧ {} / 9</p>{}</a>"#,
                p.slug,
                esc(&p.title),
                esc(&p.goal),
                path_view(&rated),
                rated.len(),
                readiness(&rated),
                tells(told(data, std::slice::from_ref(&p.id), &p.id, &ms))
            );
        }
    }
    let ids: Vec<String> = projects.iter().map(|p| s(p, "id")).collect();
    let mut parents: Vec<String> = Vec::new();
    for p in projects {
        if !parents.contains(&s(p, "parent")) {
            parents.push(s(p, "parent"));
        }
    }
    let marks_of = |p: &Row| {
        let pid = s(p, "id");
        told(
            data,
            std::slice::from_ref(&pid),
            &pid,
            &milestones_of(&of(data, "milestones", std::slice::from_ref(&pid))),
        )
    };
    let whole = projects
        .iter()
        .filter(|p| marks_of(p).iter().all(|t| *t))
        .count();
    let _ = write!(
        b,
        r#"<h2>⟦every-project⟧</h2><p class="lead">{whole} / {} ⟦tell-their-whole-essence⟧</p>"#,
        projects.len()
    );
    for par in parents.iter().filter(|p| !ids.contains(p)) {
        let _ = write!(b, "<h3>{}</h3><div class=\"cards\">", esc(par));
        for p in projects.iter().filter(|p| s(p, "parent") == *par) {
            let tables = p
                .get("tables")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let _ = write!(
                b,
                r#"<div class="card"><h4><a href="{}">{}</a></h4><span class="chip">{}</span><span class="chip">{tables} shared tables</span><p>{}</p>{}</div>"#,
                esc(&s(p, "link")),
                esc(&s(p, "name")),
                esc(&s(p, "kind")),
                esc(&s(p, "id")),
                tells(marks_of(p))
            );
        }
        b.push_str("</div>");
    }
    let mut log: Vec<&Row> = data.rows("session_log").iter().collect();
    log.sort_by_key(|r| std::cmp::Reverse(s(r, "entry_date")));
    b.push_str("<h2>⟦latest-work⟧</h2>");
    for r in log.into_iter().take(4) {
        let _ = write!(
            b,
            r#"<div class="log"><time>{} · {}</time><h4>{}</h4><p>{}</p></div>"#,
            esc(&s(r, "entry_date")),
            esc(&s(r, "project")),
            esc(&s(r, "title")),
            esc(&cut(&s(r, "done"), 280))
        );
    }
    b
}

/// The test runs as stacked bars - passed, failed, skipped - oldest first.
pub fn runs_chart(runs: &[&Row]) -> String {
    let mut runs = runs.to_vec();
    runs.sort_by_key(|r| s(r, "run_at"));
    let (w, label, h) = (720.0_f64, 170.0_f64, 26.0_f64);
    let mut svg = format!(
        r#"<svg viewBox="0 0 {w} {}" width="100%" role="img" aria-label="test runs">"#,
        (runs.len() as f64) * (h + 10.0) + 10.0
    );
    for (i, r) in runs.iter().enumerate() {
        let (p, f, k) = (
            n(r, "passed") as f64,
            n(r, "failed") as f64,
            n(r, "skipped") as f64,
        );
        let total = (p + f + k).max(1.0);
        let y = 5.0 + i as f64 * (h + 10.0);
        let _ = write!(
            svg,
            r#"<text x="0" y="{}" font-size="13" fill="currentColor" opacity=".65">{}</text>"#,
            y + 17.0,
            esc(&cut(&s(r, "run_at"), 16))
        );
        let mut x = label;
        for (v, c, what) in [
            (p, "var(--good)", "passed"),
            (f, "var(--bad)", "failed"),
            (k, "var(--none)", "skipped"),
        ] {
            let bw = (w - label) * v / total;
            if bw > 0.0 {
                let _ = write!(
                    svg,
                    r#"<rect x="{x:.1}" y="{y}" width="{bw:.1}" height="{h}" rx="5" fill="{c}"><title>{} {what}</title></rect>"#,
                    v as i64
                );
            }
            x += bw;
        }
    }
    svg + "</svg>"
}

fn backlog(rows: &[&Row]) -> String {
    let mut b = String::from(
        r#"<h1>⟦view-backlog⟧</h1><p class="lead">⟦backlog-lead⟧</p><div class="board" style="margin-top:32px">"#,
    );
    for (st, title) in [
        ("open", "⟦open⟧"),
        ("in-progress", "⟦in-progress⟧"),
        ("done", "⟦done⟧"),
    ] {
        let items: Vec<&&Row> = rows.iter().filter(|r| s(r, "status") == st).collect();
        let _ = write!(b, r#"<div class="col"><h3>{title} · {}</h3>"#, items.len());
        for r in items {
            let proj = s(r, "project");
            let short = proj.rsplit('/').next().unwrap_or(&proj).to_string();
            let _ = write!(
                b,
                r#"<div class="card"><h4>{}</h4><span class="chip">{}</span><p>{}</p></div>"#,
                esc(&s(r, "title")),
                esc(&short),
                esc(&cut(&s(r, "note"), 200))
            );
        }
        b.push_str("</div>");
    }
    b + "</div>"
}

/// A stop on a path: its horizon, readiness level and criteria (see `evaluate`).
#[derive(Debug, Clone)]
pub struct Milestone {
    pub id: String,
    pub subject: String,
    pub ord: i64,
    pub title: String,
    pub horizon: String,
    pub readiness: i64,
    pub criteria: Vec<String>,
    pub why: String,
    /// the project whose milestone this is: its criteria are read there first
    pub project: String,
}

fn milestones_of(rows: &[&Row]) -> Vec<Milestone> {
    let mut ms: Vec<Milestone> = rows
        .iter()
        .map(|r| Milestone {
            id: s(r, "id"),
            subject: s(r, "subject"),
            ord: n(r, "ord"),
            title: s(r, "title"),
            horizon: s(r, "horizon"),
            readiness: n(r, "readiness"),
            criteria: s(r, "criteria")
                .split_whitespace()
                .map(String::from)
                .collect(),
            why: s(r, "why"),
            project: s(r, "project"),
        })
        .collect();
    ms.sort_by_key(|m| (m.subject.clone(), m.ord));
    ms
}

/// One criterion, read from the data: (what, status) for each thing it names - `feature:ID@PLATFORM`,
/// `component:ID@DEVICE` (`*` for every one), `tests@DEVICE` (the latest run), `backlog:ID`.
pub fn evaluate(data: &Data, project: &str, criterion: &str) -> Vec<(String, String)> {
    let status_in = |table: &str, a: &str, av: &str, b: &str, bv: &str| -> String {
        data.rows(table)
            .iter()
            .find(|r| s(r, a) == av && s(r, b) == bv)
            .map(|r| s(r, "status"))
            .unwrap_or_else(|| "unknown".into())
    };
    let every =
        |table: &str| -> Vec<String> { data.rows(table).iter().map(|r| s(r, "id")).collect() };
    if let Some(rest) = criterion.strip_prefix("feature:") {
        let (id, at) = rest.split_once('@').unwrap_or((rest, ""));
        let ids = if id == "*" {
            every("features")
        } else {
            vec![id.to_string()]
        };
        return ids
            .into_iter()
            .map(|f| {
                let st = status_in("feature_support", "feature_id", &f, "platform_id", at);
                (f, st)
            })
            .collect();
    }
    if let Some(rest) = criterion.strip_prefix("component:") {
        let (id, at) = rest.split_once('@').unwrap_or((rest, ""));
        let ids = if id == "*" {
            every("components")
        } else {
            vec![id.to_string()]
        };
        return ids
            .into_iter()
            .map(|c| {
                let st = status_in("support_status", "component_id", &c, "device_id", at);
                (c, st)
            })
            .collect();
    }
    if let Some(dev) = criterion.strip_prefix("tests@") {
        let last = data
            .rows("test_runs")
            .iter()
            .filter(|r| s(r, "device_id") == dev)
            .max_by_key(|r| s(r, "run_at"));
        let st = match last {
            None => "unknown",
            Some(r) if n(r, "failed") == 0 => "works",
            Some(_) => "broken",
        };
        return vec![(format!("tests on {dev}"), st.into())];
    }
    if let Some(id) = criterion.strip_prefix("backlog:") {
        let st = match data
            .rows("backlog")
            .iter()
            // in the milestone's own project first; then anywhere - a path may read another project's work
            .find(|r| s(r, "id") == id && s(r, "project") == project)
            .or_else(|| data.rows("backlog").iter().find(|r| s(r, "id") == id))
            .map(|r| s(r, "status"))
        {
            Some(x) if x == "done" => "works",
            Some(x) if x == "in-progress" => "partial",
            Some(_) => "open",
            None => "unknown",
        };
        return vec![(id.to_string(), st.into())];
    }
    vec![(criterion.to_string(), "unknown".into())]
}

/// A milestone's evidence: good, partly, not yet, unknown - and what that makes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evidence {
    pub good: usize,
    pub part: usize,
    pub bad: usize,
    pub unknown: usize,
}

impl Evidence {
    fn total(&self) -> usize {
        self.good + self.part + self.bad + self.unknown
    }
    /// done, in progress, not started, or unknown (nothing measured)
    pub fn state(&self) -> &'static str {
        let t = self.total();
        if t > 0 && self.good == t {
            "done"
        } else if self.unknown == t {
            "unknown"
        } else if self.good + self.part > 0 {
            "in progress"
        } else {
            "not started"
        }
    }
    fn share(&self) -> f64 {
        (self.good as f64 + self.part as f64 * 0.5) / self.total().max(1) as f64
    }
}

pub fn evidence(items: &[(String, String)]) -> Evidence {
    let mut e = Evidence {
        good: 0,
        part: 0,
        bad: 0,
        unknown: 0,
    };
    for (_, st) in items {
        match tone(st) {
            "good" => e.good += 1,
            "part" => e.part += 1,
            "bad" | "todo" => e.bad += 1,
            "na" => {}
            _ => e.unknown += 1,
        }
    }
    e
}

const HORIZONS: [(&str, &str); 3] = [
    ("working", "⟦horizon-working⟧"),
    ("perfection", "⟦horizon-perfection⟧"),
    ("ongoing", "⟦horizon-ongoing⟧"),
];

/// The path: three horizons, a stop per milestone with its evidence, the way travelled so far, and
/// the readiness ladder.
pub fn path_view(ms: &[(Milestone, Evidence)]) -> String {
    // Words are laid out by the browser, never placed in a drawing: each stop is a cell of its own,
    // its label ordinary text that wraps inside the cell and cannot run into its neighbour (text in
    // SVG cannot wrap and knows nothing of what is beside it). The drawings hold shapes only.
    let in_order: Vec<&(Milestone, Evidence)> = HORIZONS
        .iter()
        .flat_map(|(h, _)| ms.iter().filter(move |(m, _)| m.horizon == *h))
        .collect();
    // the road is travelled only as far as every stop before it is done
    let run = in_order
        .iter()
        .take_while(|(_, e)| e.state() == "done")
        .count();
    let circumference = 2.0 * std::f64::consts::PI * 15.0;
    let mut out = String::from(r#"<div class="path"><div class="bands">"#);
    let mut index = 0;
    for (h, label) in HORIZONS {
        let in_h: Vec<&(Milestone, Evidence)> = ms.iter().filter(|(m, _)| m.horizon == h).collect();
        let _ = write!(
            out,
            r#"<div class="band" style="flex:{} 1 0"><div class="hlabel">{label}</div><div class="stops">"#,
            in_h.len().max(1)
        );
        for (m, e) in in_h {
            let tone = match e.state() {
                "done" => "good",
                "in progress" => "part",
                "not started" => "todo",
                _ => "none",
            };
            let _ = write!(
                out,
                r#"<div class="stopcell{}" title="{}: {} - {} of {} criteria good, {} partly, {} not yet, {} unknown"><svg viewBox="0 0 40 40" width="40" height="40" aria-hidden="true"><circle cx="20" cy="20" r="15" class="stop"/><circle cx="20" cy="20" r="15" class="ring {tone}" stroke-dasharray="{:.1} {circumference:.1}" transform="rotate(-90 20 20)"/></svg><span class="frac">{}/{}</span><span class="stitle">{}</span></div>"#,
                if index < run { " passed" } else { "" },
                esc(&m.title),
                e.state(),
                e.good,
                e.total(),
                e.part,
                e.bad,
                e.unknown,
                circumference * e.share(),
                e.good,
                e.total(),
                esc(&m.title)
            );
            index += 1;
        }
        out.push_str("</div></div>");
    }
    out.push_str("</div>");
    // readiness is earned in order: the highest level in the unbroken run of done milestones
    let level = readiness(ms);
    let level_text = esc(&crate::i18n::tn("level-of-9", level));
    out.push_str(
        r#"<div class="ladder"><span class="hlabel">⟦readiness⟧</span><span class="steps">"#,
    );
    for l in 1..=9 {
        let _ = write!(
            out,
            r#"<span class="{}" title="readiness level {l}"></span>"#,
            if l <= level { "lvl on" } else { "lvl" }
        );
    }
    let _ = write!(
        out,
        r#"</span><span class="hlabel">{level_text}</span></div></div>"#
    );
    out
}

/// The readiness a path has earned: the highest level among its milestones done in an unbroken run
/// from the start - a later milestone done early does not lift it.
pub fn readiness(ms: &[(Milestone, Evidence)]) -> i64 {
    let mut ordered: Vec<&(Milestone, Evidence)> = ms.iter().collect();
    let place = |h: &str| HORIZONS.iter().position(|(k, _)| *k == h).unwrap_or(3);
    ordered.sort_by_key(|(m, _)| (place(&m.horizon), m.ord));
    ordered
        .iter()
        .take_while(|(_, e)| e.state() == "done")
        .map(|(m, _)| m.readiness)
        .max()
        .unwrap_or(0)
}

/// A subject's path and the evidence of each of its milestones.
fn path_section(data: &Data, title: &str, ms: &[Milestone]) -> String {
    let rated: Vec<(Milestone, Evidence)> = ms
        .iter()
        .map(|m| {
            let items: Vec<(String, String)> = m
                .criteria
                .iter()
                .flat_map(|c| evaluate(data, &m.project, c))
                .collect();
            (m.clone(), evidence(&items))
        })
        .collect();
    let mut b = format!(
        r#"<h2>{}</h2><div class="pathbox">{}</div><div class="cards" style="margin-top:16px">"#,
        esc(title),
        path_view(&rated)
    );
    for (m, e) in &rated {
        let items: Vec<(String, String)> = m
            .criteria
            .iter()
            .flat_map(|c| evaluate(data, &m.project, c))
            .collect();
        let mut signs = String::new();
        for (what, st) in items.iter().take(60) {
            let tn = tone(st);
            let _ = write!(
                signs,
                r#"<span class="mini {tn}" title="{}: {}">{}</span>"#,
                esc(what),
                esc(st),
                sign(tn)
            );
        }
        let h = HORIZONS
            .iter()
            .find(|(k, _)| *k == m.horizon)
            .map(|(_, l)| *l)
            .unwrap_or("");
        let _ = write!(
            b,
            r#"<div class="card" id="{}"><h4>{}</h4><span class="chip">{h}</span><span class="chip">⟦readiness-chip⟧ {}</span><span class="chip">⟦state-{}⟧</span><p>{}</p><p class="signs">{signs}</p></div>"#,
            esc(&m.id),
            esc(&m.title),
            m.readiness,
            e.state().replace(' ', "-"),
            esc(&m.why)
        );
    }
    b + "</div>"
}

fn first(r: &Row, keys: &[&str]) -> String {
    keys.iter()
        .map(|k| s(r, k))
        .find(|x| !x.is_empty())
        .unwrap_or_default()
}

fn from(r: &Row) -> String {
    let p = s(r, "project");
    p.rsplit('/').next().unwrap_or(&p).to_string()
}

/// How the commons works: the canon's principles and architectures, every practice and concept -
/// each card naming the project it comes from.
fn how_we_work(data: &Data) -> String {
    let mut b = String::from(r#"<h1>⟦nav-how-we-work⟧</h1><p class="lead">⟦how-we-work-lead⟧</p>"#);
    let principles = data.rows("principles");
    if !principles.is_empty() {
        b.push_str("<h2>⟦principles⟧</h2>");
        let mut cats: Vec<String> = Vec::new();
        for r in principles {
            if !cats.contains(&s(r, "category")) {
                cats.push(s(r, "category"));
            }
        }
        for c in cats {
            let _ = write!(b, r#"<h3>{}</h3><div class="cards">"#, esc(&c));
            for r in principles.iter().filter(|r| s(r, "category") == c) {
                let origin = [s(r, "origin_author"), s(r, "origin_year")]
                    .into_iter()
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = write!(
                    b,
                    r#"<div class="card"><h4>{}</h4><p>{}</p>{}<p>{}<span class="chip">{}</span></p></div>"#,
                    esc(&s(r, "name")),
                    esc(&s(r, "statement")),
                    if s(r, "caveat").is_empty() {
                        String::new()
                    } else {
                        format!(r#"<p><b>⟦but⟧</b> {}</p>"#, esc(&s(r, "caveat")))
                    },
                    if origin.is_empty() {
                        String::new()
                    } else {
                        format!(r#"<span class="chip">{}</span>"#, esc(&origin))
                    },
                    esc(&from(r))
                );
            }
            b.push_str("</div>");
        }
    }
    let arch = data.rows("architectures");
    if !arch.is_empty() {
        b.push_str(r#"<h2>⟦architectures⟧</h2><div class="cards">"#);
        for r in arch {
            let _ = write!(
                b,
                r#"<div class="card"><h4>{}</h4><p>{}</p><p><b>⟦best-when⟧</b> {}</p><p><span class="chip">{}</span></p></div>"#,
                esc(&s(r, "name")),
                esc(&s(r, "essence")),
                esc(&s(r, "best_when")),
                esc(&from(r))
            );
        }
        b.push_str("</div>");
    }
    for (table, title) in [("practices", "⟦practices⟧"), ("concepts", "⟦concepts⟧")] {
        let rows = data.rows(table);
        if rows.is_empty() {
            continue;
        }
        let _ = write!(b, r#"<h2>{title}</h2><div class="cards">"#);
        for r in rows {
            let who = s(r, "for");
            let _ = write!(
                b,
                r#"<div class="card"><h4>{}</h4><p>{}</p><p>{}<span class="chip">{}</span></p></div>"#,
                esc(&cut(&first(r, &["name", "practice"]), 160)),
                esc(&first(
                    r,
                    &["summary", "description", "definition", "why", "statement"]
                )),
                if who.is_empty() {
                    String::new()
                } else {
                    format!(r#"<span class="chip">for {}</span>"#, esc(&who))
                },
                esc(&from(r))
            );
        }
        b.push_str("</div>");
    }
    b
}

/// Milestones with the evidence each has.
fn rate(data: &Data, ms: &[Milestone]) -> Vec<(Milestone, Evidence)> {
    ms.iter()
        .map(|m| {
            let items: Vec<(String, String)> = m
                .criteria
                .iter()
                .flat_map(|c| evaluate(data, &m.project, c))
                .collect();
            (m.clone(), evidence(&items))
        })
        .collect()
}

/// A project with a path of its own: every marked project that exports milestones and is not a
/// grand project (those have their tabs).
pub struct Sub {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub slug: String,
    pub link: String,
}

pub fn subs(data: &Data) -> Vec<Sub> {
    // siblings in the order their marks give (lower first), then by name
    let mut rows: Vec<&Row> = data.rows("projects").iter().collect();
    rows.sort_by_key(|p| {
        (
            p.get("order").and_then(Value::as_i64).unwrap_or(i64::MAX),
            s(p, "name"),
        )
    });
    rows.into_iter()
        .filter(|p| {
            s(p, "kind") != "picture"
                && data
                    .rows("milestones")
                    .iter()
                    .any(|m| s(m, "project") == s(p, "id"))
        })
        .map(|p| {
            let name = s(p, "name");
            let (before, after) = name
                .split_once(':')
                .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
                .unwrap_or((name.clone(), String::new()));
            let title = Some(s(p, "title"))
                .filter(|t| !t.is_empty())
                .unwrap_or(before);
            Sub {
                slug: format!(
                    "project-{}",
                    Some(s(p, "slug"))
                        .filter(|x| !x.is_empty())
                        .unwrap_or_else(|| slug(&title))
                ),
                id: s(p, "id"),
                goal: after,
                title,
                link: s(p, "link"),
            }
        })
        .collect()
}

const SUB_VIEWS: [(&str, &str); 7] = [
    ("", "⟦view-essence⟧"),
    ("-idea", "⟦view-idea⟧"),
    ("-backlog", "⟦view-backlog⟧"),
    ("-log", "⟦view-log⟧"),
    ("-ideas", "⟦view-ideas⟧"),
    ("-words", "⟦view-words⟧"),
    ("-hand", "⟦view-hand⟧"),
];

fn sub_nav(p: &Sub, here: &str) -> String {
    let mut x = format!(r#"<div class="psub"><b>{}</b>"#, esc(&p.title));
    for (suffix, label) in SUB_VIEWS {
        let _ = write!(
            x,
            r#"<a href="{}{suffix}.html"{}>{label}</a>"#,
            p.slug,
            if suffix == here { r#" class="on""# } else { "" }
        );
    }
    let _ = write!(x, r#"<a href="{}">⟦repository⟧ ↗</a></div>"#, esc(&p.link));
    x
}

/// A project's own pages: how close it is (its path), its backlog, its log, its ideas.
pub fn sub_pages(data: &Data, p: &Sub) -> Vec<(String, String, String)> {
    let fam = vec![p.id.clone()];
    let ms = milestones_of(&of(data, "milestones", &fam));
    let back = of(data, "backlog", &fam);
    // a project lands on its essence: the whole chain on one page
    let how = format!(
        "{}{}",
        sub_nav(p, ""),
        essence_html(data, &fam, &p.id, &ms, &p.title, &p.goal)
    );
    let mut logs = of(data, "session_log", &fam);
    logs.sort_by_key(|r| std::cmp::Reverse(s(r, "entry_date")));
    let mut log = format!(
        r#"{}<h1>⟦view-log⟧</h1><p class="lead">⟦log-lead⟧</p>"#,
        sub_nav(p, "-log")
    );
    for r in logs {
        let rejected = if s(r, "rejected").is_empty() {
            String::new()
        } else {
            format!(r#"<p><b>⟦rejected⟧</b> {}</p>"#, esc(&s(r, "rejected")))
        };
        let _ = write!(
            log,
            r#"<div class="log"><time>{}</time><h4>{}</h4><p>{}</p>{rejected}</div>"#,
            esc(&s(r, "entry_date")),
            esc(&s(r, "title")),
            esc(&s(r, "done"))
        );
    }
    let mut ideas = format!(
        r#"{}<h1>⟦view-ideas⟧</h1><p class="lead">⟦ideas-lead⟧</p><div class="cards">"#,
        sub_nav(p, "-ideas")
    );
    for r in of(data, "concepts", &fam) {
        let _ = write!(
            ideas,
            r#"<div class="card"><h4>{}</h4><p>{}</p></div>"#,
            esc(&s(r, "name")),
            esc(&first(r, &["summary", "statement", "description"]))
        );
    }
    for r in of(data, "decisions", &fam) {
        let _ = write!(
            ideas,
            r#"<div class="card"><h4>{}</h4><p>{}</p><p><b>⟦rejected⟧</b> {}</p><p><span class="chip">{}</span></p></div>"#,
            esc(&s(r, "decision")),
            esc(&s(r, "why")),
            esc(&s(r, "rejected")),
            esc(&s(r, "date"))
        );
    }
    ideas.push_str("</div>");
    vec![
        (format!("{}.html", p.slug), p.title.clone(), how),
        (
            format!("{}-hand.html", p.slug),
            format!("⟦view-hand⟧ · {}", p.title),
            format!("{}{}", sub_nav(p, "-hand"), handwork_html(data, &fam)),
        ),
        (
            format!("{}-words.html", p.slug),
            format!("⟦view-words⟧ · {}", p.title),
            format!("{}{}", sub_nav(p, "-words"), vocabulary_html(data, &fam)),
        ),
        (
            format!("{}-idea.html", p.slug),
            format!("⟦view-idea⟧ · {}", p.title),
            format!(
                "{}{}",
                sub_nav(p, "-idea"),
                mark_words(&idea_html(&of(data, "idea", &fam)), &own_words(data, &fam))
            ),
        ),
        (
            format!("{}-backlog.html", p.slug),
            format!("Backlog · {}", p.title),
            format!("{}{}", sub_nav(p, "-backlog"), backlog(&back)),
        ),
        (
            format!("{}-log.html", p.slug),
            format!("Log · {}", p.title),
            log,
        ),
        (
            format!("{}-ideas.html", p.slug),
            format!("Ideas · {}", p.title),
            ideas,
        ),
    ]
}

/// The observatory's own family: the projects with a path that the observatory's organization owns.
const HOME: &str = "open-commons-observatory/";

fn family_bar(data: &Data, here: &str) -> String {
    let mut x = String::from(r#"<div class="psub"><b>⟦nav-observatory⟧</b>"#);
    let _ = write!(
        x,
        r#"<a href="index.html"{}>⟦overview⟧</a>"#,
        if here == "index.html" {
            r#" class="on""#
        } else {
            ""
        }
    );
    for p in subs(data).iter().filter(|p| p.id.starts_with(HOME)) {
        let _ = write!(
            x,
            r#"<a href="{}.html"{}>{}</a>"#,
            p.slug,
            if here.starts_with(&p.slug) {
                r#" class="on""#
            } else {
                ""
            },
            esc(&p.title)
        );
    }
    x + "</div>"
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// What the work itself says, gathered by ocoeco: commits per day over 90 days, the latest
/// release, the state of each CI workflow. Empty when nothing was gathered.
pub fn made_by_work(data: &Data, fam: &[String], now: i64) -> String {
    let act = of(data, "activity", fam);
    let rel = of(data, "releases", fam);
    let runs = of(data, "ci", fam);
    if act.is_empty() && rel.is_empty() && runs.is_empty() {
        return String::new();
    }
    let days: Vec<String> = (0..90)
        .rev()
        .map(|k| utc(now - k * 86_400)[..10].to_string())
        .collect();
    let mut per: BTreeMap<String, i64> = BTreeMap::new();
    for r in &act {
        *per.entry(s(r, "id")).or_default() += n(r, "commits");
    }
    let counts: Vec<i64> = days
        .iter()
        .map(|d| per.get(d).copied().unwrap_or(0))
        .collect();
    let total: i64 = counts.iter().sum();
    let max = counts.iter().copied().max().unwrap_or(0).max(1);
    let mut svg = String::from(
        r#"<svg class="spark" viewBox="0 0 900 60" width="100%" role="img" aria-label="commits per day">"#,
    );
    for (i, (d, c)) in days.iter().zip(&counts).enumerate() {
        let h = if *c == 0 {
            2.0
        } else {
            4.0 + 52.0 * *c as f64 / max as f64
        };
        let _ = write!(
            svg,
            r#"<rect x="{:.1}" y="{:.1}" width="7" height="{h:.1}" rx="2" class="{}"><title>{d}: {c} commits</title></rect>"#,
            i as f64 * 10.0,
            58.0 - h,
            if *c == 0 { "idle" } else { "busy" }
        );
    }
    svg.push_str("</svg>");
    let release = rel
        .iter()
        .max_by_key(|r| s(r, "published"))
        .map(|r| {
            format!(
                r#"<a class="chip" href="{}">⟦latest-release⟧ {} · {}</a>"#,
                esc(&s(r, "link")),
                esc(&s(r, "id")),
                esc(&s(r, "published"))
            )
        })
        .unwrap_or_default();
    let mut cis = String::new();
    for r in &runs {
        let c = s(r, "conclusion");
        let t = match c.as_str() {
            "success" => "good",
            "failure" | "cancelled" | "timed_out" | "startup_failure" => "bad",
            "in_progress" | "queued" | "waiting" => "part",
            _ => "none",
        };
        let short = s(r, "project");
        let short = short.rsplit('/').next().unwrap_or("");
        let label = if fam.len() > 1 {
            format!("{short}: {}", s(r, "id"))
        } else {
            s(r, "id")
        };
        let _ = write!(
            cis,
            r#"<a class="chip ci {t}" href="{}" title="{}: {c}">{} {}</a>"#,
            esc(&s(r, "link")),
            esc(&label),
            sign(t),
            esc(&label)
        );
    }
    let work_line = esc(&crate::i18n::tn("commits-90", total));
    format!(
        r#"<h2>⟦made-by-work⟧</h2><div class="pathbox"><p class="work"><b>{work_line}</b> {release}</p><p class="work">{cis}</p>{svg}</div>"#
    )
}

/// Interface text is marked with its message id where it is written - ⟦message-id⟧ - and each page
/// resolves its marks in its own language (messages from the Fluent catalogs, escaped).
pub fn localize(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find('⟦') {
        out.push_str(&rest[..i]);
        let after = &rest[i + '⟦'.len_utf8()..];
        match after.find('⟧') {
            Some(j) => {
                out.push_str(&esc(&t(&after[..j])));
                rest = &after[j + '⟧'.len_utf8()..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// A passage of prose: paragraphs, and **bold** - made safe first.
fn rich(text: &str) -> String {
    text.split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let escaped = esc(p.trim());
            let mut out = String::new();
            let mut bold = false;
            for (i, part) in escaped.split("**").enumerate() {
                if i > 0 {
                    out.push_str(if bold { "</b>" } else { "<b>" });
                    bold = !bold;
                }
                out.push_str(part);
            }
            if bold {
                out.push_str("</b>");
            }
            format!("<p>{out}</p>")
        })
        .collect()
}

/// A project's idea, as it wrote it: an ordered piece of writing for anyone - the opening, the
/// problem, the idea, a picture, the honest limit, where it stands - shown as an article to read.
pub fn idea_html(rows: &[&Row]) -> String {
    if rows.is_empty() {
        return r#"<p class="lead">⟦idea-not-written⟧</p>"#.to_string();
    }
    let mut rows = rows.to_vec();
    rows.sort_by_key(|r| n(r, "ord"));
    let mut b = String::from(r#"<article class="idea">"#);
    for r in rows {
        let (heading, text) = (s(r, "heading"), rich(&s(r, "text")));
        match s(r, "kind").as_str() {
            "lead" => {
                let _ = write!(b, r#"<div class="first">{text}</div>"#);
            }
            "picture" => {
                let title = if heading.is_empty() {
                    String::new()
                } else {
                    format!("<b>{}</b>", esc(&heading))
                };
                let _ = write!(b, "<blockquote>{title}{text}</blockquote>");
            }
            "limit" => {
                let _ = write!(
                    b,
                    r#"<div class="limit"><h3>{}</h3>{text}</div>"#,
                    esc(&heading)
                );
            }
            _ => {
                if !heading.is_empty() {
                    let _ = write!(b, "<h2>{}</h2>", esc(&heading));
                }
                b.push_str(&text);
            }
        }
    }
    b + "</article>"
}

/// ocoeco's own project: its vocabulary holds the shared words every project speaks.
const LANGUAGE: &str = "open-commons-observatory/ocoeco";

/// A family's own words (not ocoeco's shared ones): term and meaning.
fn own_words(data: &Data, fam: &[String]) -> Vec<(String, String)> {
    of(data, "vocabulary", fam)
        .into_iter()
        .filter(|r| s(r, "project") != LANGUAGE)
        .map(|r| (s(r, "term"), s(r, "means")))
        .collect()
}

/// Marks the first use of each of a project's own words in a piece of writing, with its meaning beside
/// it. Only its own words: ocoeco's shared words are often ordinary English, used in prose in their
/// ordinary sense.
pub fn mark_words(html: &str, words: &[(String, String)]) -> String {
    let mut out = html.to_string();
    let mut ws: Vec<&(String, String)> =
        words.iter().filter(|(t, _)| !t.trim().is_empty()).collect();
    ws.sort_by_key(|(t, _)| std::cmp::Reverse(t.len()));
    for (term, means) in ws {
        let low = out.to_lowercase();
        if low.len() != out.len() {
            break;
        }
        let t = term.to_lowercase();
        let mut from = 0;
        while let Some(i) = low[from..].find(&t) {
            let (at, end) = (from + i, from + i + t.len());
            let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
            let in_tag = low[..at].rfind('<') > low[..at].rfind('>');
            let in_word = low[..at].rfind("<abbr") > low[..at].rfind("</abbr>");
            if edge(low[..at].chars().next_back())
                && edge(low[end..].chars().next())
                && !in_tag
                && !in_word
                && out.is_char_boundary(at)
                && out.is_char_boundary(end)
            {
                out = format!(
                    r#"{}<abbr class="word" title="{}">{}</abbr>{}"#,
                    &out[..at],
                    esc(means),
                    &out[at..end],
                    &out[end..]
                );
                break;
            }
            from = end;
        }
    }
    out
}

/// The words a project or a family speaks: its own, then ocoeco's shared ones - each with what it means,
/// what it is not, and related words.
pub fn vocabulary_html(data: &Data, fam: &[String]) -> String {
    let card = |r: &&Row| {
        let (term, not, see) = (s(r, "term"), s(r, "not"), s(r, "see"));
        let mut c = format!(
            r#"<div class="card" id="w-{}"><h4><dfn>{}</dfn></h4><p>{}</p>"#,
            slug(&term),
            esc(&term),
            esc(&s(r, "means"))
        );
        if !not.is_empty() {
            let _ = write!(c, "<p><b>⟦words-not⟧</b> {}</p>", esc(&not));
        }
        if !see.is_empty() {
            let links: Vec<String> = see
                .split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .map(|x| format!(r##"<a href="#w-{}">{}</a>"##, slug(x), esc(x)))
                .collect();
            let _ = write!(c, "<p><b>⟦words-see⟧</b> {}</p>", links.join(", "));
        }
        c + "</div>"
    };
    let mut own: Vec<&Row> = of(data, "vocabulary", fam)
        .into_iter()
        .filter(|r| s(r, "project") != LANGUAGE)
        .collect();
    let language = [LANGUAGE.to_string()];
    let mut shared = of(data, "vocabulary", &language);
    own.sort_by_key(|r| s(r, "term").to_lowercase());
    shared.sort_by_key(|r| s(r, "term").to_lowercase());
    let mut b = String::from(r#"<h1>⟦view-words⟧</h1><p class="lead">⟦words-lead⟧</p>"#);
    if !own.is_empty() {
        b.push_str(r#"<h2>⟦words-own⟧</h2><div class="cards">"#);
        b.extend(own.iter().map(card));
        b.push_str("</div>");
    }
    b.push_str(r#"<h2>⟦words-shared⟧</h2><div class="cards">"#);
    b.extend(shared.iter().map(card));
    b + "</div>"
}

/// The small actions a project does by hand, and how often - as bars. Past three, a manual one is
/// saturated and asks for a tool; an automated one names its tool; a deliberate one only counts.
pub fn handwork_html(data: &Data, fam: &[String]) -> String {
    let mut rows = of(data, "handwork", fam);
    rows.sort_by_key(|r| std::cmp::Reverse(n(r, "count")));
    let max = rows.iter().map(|r| n(r, "count")).max().unwrap_or(1).max(1);
    let mut b = String::from(r#"<h1>⟦view-hand⟧</h1><p class="lead">⟦hand-lead⟧</p>"#);
    if rows.is_empty() {
        return b + r#"<p class="mute">⟦attention-none⟧</p>"#;
    }
    b.push_str(r#"<div class="hand">"#);
    for r in rows {
        let (c, st, by) = (n(r, "count"), s(r, "status"), s(r, "automated_by"));
        let kind = match st.as_str() {
            "manual" | "automated" | "deliberate" => st.clone(),
            _ => "manual".to_string(),
        };
        let sat = kind == "manual" && c >= 3;
        let _ = write!(
            b,
            r#"<div class="hrow{}"><span class="hact">{}{}</span><span class="hbarbox"><span class="hbar {kind}" style="width:{}%"></span></span><b>{c}×</b><span class="chip">⟦hand-status-{kind}⟧{}</span></div>"#,
            if sat { " sat" } else { "" },
            esc(&s(r, "action")),
            if sat {
                r#" <span class="chip warn">⟦hand-worth⟧</span>"#
            } else {
                ""
            },
            (c * 100 / max).max(3),
            if by.is_empty() {
                String::new()
            } else {
                format!(" · {}", esc(&by))
            }
        );
    }
    b + "</div>"
}

/// Where the whole is weakest right now, read from every project's data: what waits on a person,
/// steps that nothing delivers, and work that has gone quiet. Read before choosing the next piece.
pub fn attention_html(data: &Data) -> String {
    let page: std::collections::HashMap<String, String> = subs(data)
        .into_iter()
        .map(|p| (p.id.clone(), p.slug.clone()))
        .collect();
    let name = |id: &str| {
        let t = data
            .rows("projects")
            .iter()
            .find(|p| s(p, "id") == id)
            .map(|p| first(p, &["title", "name"]))
            .unwrap_or_else(|| id.to_string());
        match page.get(id) {
            Some(sl) => format!(r#"<a href="{}.html">{}</a>"#, sl, esc(&t)),
            None => esc(&t),
        }
    };
    let list = |items: Vec<String>| {
        if items.is_empty() {
            r#"<p class="mute">⟦attention-none⟧</p>"#.to_string()
        } else {
            format!(
                "<ul>{}</ul>",
                items
                    .iter()
                    .take(7)
                    .map(|i| format!("<li>{i}</li>"))
                    .collect::<String>()
            )
        }
    };
    let waiting: Vec<String> = data
        .rows("backlog")
        .iter()
        .filter(|r| s(r, "status") != "done" && !s(r, "waits").is_empty())
        .map(|r| {
            format!(
                "{} · {} <span class=\"chip\">{}</span>",
                name(&s(r, "project")),
                esc(&s(r, "title")),
                esc(&s(r, "waits"))
            )
        })
        .collect();
    let mut unserved: Vec<(usize, String)> = Vec::new();
    let ids: Vec<String> = data.rows("projects").iter().map(|p| s(p, "id")).collect();
    for id in &ids {
        let one = std::slice::from_ref(id);
        let ms = milestones_of(&of(data, "milestones", one));
        if !of(data, "backlog", one)
            .iter()
            .any(|r| s(r, "status") == "in-progress")
        {
            continue; // attention is for projects being worked on: a parked one waits by choice
        }
        let workings = of(data, "workings", one);
        let open: Vec<String> = rate(data, &ms)
            .into_iter()
            .filter(|(m, e)| {
                e.state() != "done" && !workings.iter().any(|w| s(w, "serves") == m.id)
            })
            .map(|(m, _)| m.title)
            .collect();
        if !open.is_empty() {
            unserved.push((
                open.len(),
                format!(
                    "{} · {}: {}",
                    name(id),
                    open.len(),
                    esc(&open.iter().take(3).cloned().collect::<Vec<_>>().join(", "))
                ),
            ));
        }
    }
    unserved.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    let week = utc(now_secs() - 7 * 86_400)
        .chars()
        .take(10)
        .collect::<String>();
    let mut quiet: Vec<String> = Vec::new();
    for id in &ids {
        let one = std::slice::from_ref(id);
        let doing = of(data, "backlog", one)
            .iter()
            .filter(|r| s(r, "status") == "in-progress")
            .count();
        if doing == 0 {
            continue;
        }
        if of(data, "activity", one).is_empty() {
            continue; // not gathered yet: unknown, not quiet
        }
        let last = of(data, "activity", one)
            .iter()
            .filter(|r| n(r, "commits") > 0)
            .map(|r| s(r, "id"))
            .max()
            .unwrap_or_default();
        if last < week {
            quiet.push(format!(
                "{} · {doing} ⟦attention-in-progress⟧, ⟦attention-last⟧ {}",
                name(id),
                if last.is_empty() {
                    "-".into()
                } else {
                    esc(&last)
                }
            ));
        }
    }
    // hand-work worth a tool: manual actions past three, summed across projects, by the time they cost
    let mut sums: Vec<(String, String, i64, i64, Vec<String>)> = Vec::new();
    for r in data
        .rows("handwork")
        .iter()
        .filter(|r| s(r, "status") == "manual")
    {
        let (id, c) = (s(r, "id"), n(r, "count"));
        match sums.iter_mut().find(|x| x.0 == id) {
            Some(x) => {
                x.2 += c;
                x.4.push(s(r, "project"));
            }
            None => sums.push((
                id,
                s(r, "action"),
                c,
                n(r, "minutes"),
                vec![s(r, "project")],
            )),
        }
    }
    sums.retain(|x| x.2 >= 3);
    sums.sort_by_key(|x| std::cmp::Reverse(x.2 * x.3.max(1)));
    let hand: Vec<String> = sums
        .iter()
        .map(|(_, a, c, m, ps)| {
            let cost = if *m > 0 {
                format!(" · ~{} ⟦minutes⟧", c * m)
            } else {
                String::new()
            };
            format!(
                "{} · {c}×{cost} · {}",
                esc(a),
                ps.iter().map(|p| name(p)).collect::<Vec<_>>().join(", ")
            )
        })
        .collect();
    format!(
        r#"<section class="attention"><h2>⟦attention⟧</h2><p class="lead">⟦attention-lead⟧</p><div class="attn"><div><h3>⟦attention-waits⟧</h3>{}</div><div><h3>⟦attention-unserved⟧</h3>{}</div><div><h3>⟦attention-quiet⟧</h3>{}</div><div><h3>⟦attention-hand⟧</h3>{}</div></div></section>"#,
        list(waiting),
        list(unserved.into_iter().map(|(_, x)| x).collect()),
        list(quiet),
        list(hand)
    )
}

/// How completely a project tells its essence - why, what, how, now - by one rule, wherever it is
/// shown: its idea is written; its path is set; every step has something that serves it; its work shows.
pub fn told(data: &Data, fam: &[String], idea_of: &str, ms: &[Milestone]) -> [bool; 4] {
    let whose = [idea_of.to_string()];
    let workings = of(data, "workings", fam);
    let served = ms
        .iter()
        .filter(|m| workings.iter().any(|w| s(w, "serves") == m.id))
        .count();
    let work = ["backlog", "session_log", "activity"]
        .iter()
        .any(|t| !of(data, t, fam).is_empty());
    [
        !of(data, "idea", &whose).is_empty(),
        !ms.is_empty(),
        !ms.is_empty() && served == ms.len(),
        work,
    ]
}

/// What kind of thing a working is, as a word from the catalog; an unknown kind is shown as written.
fn kind_word(kind: &str) -> String {
    match kind {
        "tool" | "check" | "gatherer" | "test" | "practice" => format!("⟦working-{kind}⟧"),
        other => esc(other),
    }
}

/// The four marks, small, for a card.
fn tells(marks: [bool; 4]) -> String {
    let mut b = String::from(r#"<p class="tells">"#);
    for (ok, label) in marks
        .iter()
        .zip(["⟦tell-why⟧", "⟦tell-what⟧", "⟦tell-how⟧", "⟦tell-now⟧"])
    {
        let _ = write!(
            b,
            r#"<span class="{}">{} {label}</span>"#,
            if *ok { "on" } else { "off" },
            if *ok { "✓" } else { "○" }
        );
    }
    b + "</p>"
}

/// Essence - the signature view: a project's whole chain on one page, each part flowing into the next.
/// Why it exists (the opening of its own idea) leads to what it must reach (its path); each step
/// stands with how it is delivered and proven (the workings that serve it - or, plainly, nothing
/// yet); and beneath it, what is happening now. Four marks say how completely it tells its essence.
pub fn essence_html(
    data: &Data,
    fam: &[String],
    idea_of: &str,
    ms: &[Milestone],
    title: &str,
    goal: &str,
) -> String {
    let rated = rate(data, ms);
    let done = rated.iter().filter(|(_, e)| e.state() == "done").count();
    let whose = [idea_of.to_string()];
    let mut idea_rows = of(data, "idea", &whose);
    idea_rows.sort_by_key(|r| n(r, "ord"));
    let workings = of(data, "workings", fam);
    let work = made_by_work(data, fam, now_secs());
    let doing: Vec<&Row> = of(data, "backlog", fam)
        .into_iter()
        .filter(|r| s(r, "status") == "in-progress")
        .collect();
    let marks = told(data, fam, idea_of, ms);
    let told = [
        (marks[0], "⟦essence-why⟧"),
        (marks[1], "⟦essence-what⟧"),
        (marks[2], "⟦essence-how⟧"),
        (marks[3], "⟦essence-now⟧"),
    ];
    let mut b = format!(
        r#"<h1>{}</h1><p class="lead">{}</p><div class="told">"#,
        esc(title),
        esc(goal)
    );
    for (ok, label) in told {
        let _ = write!(
            b,
            r#"<span class="tell {}">{} {label}</span>"#,
            if ok { "on" } else { "off" },
            if ok { "✓" } else { "○" }
        );
    }
    b.push_str(r#"</div><div class="flow">"#);
    let opening = idea_rows
        .first()
        .map(|r| rich(s(r, "text").split("\n\n").next().unwrap_or_default()))
        .unwrap_or_else(|| r#"<p class="mute">⟦idea-not-written⟧</p>"#.to_string());
    let _ = write!(
        b,
        r#"<section class="step" data-n="1"><h3>⟦essence-why⟧</h3><div class="opening">{opening}</div></section>"#
    );
    b.push_str(r#"<section class="step" data-n="2"><h3>⟦essence-what⟧</h3>"#);
    if rated.is_empty() {
        b.push_str(r#"<p class="mute">⟦no-path-yet⟧</p>"#);
    } else {
        let _ = write!(
            b,
            r#"<div class="pathbox">{}</div><p class="work"><b>{done} / {}</b> ⟦milestones-done⟧</p>"#,
            path_view(&rated),
            rated.len()
        );
    }
    b.push_str(r#"</section><section class="step" data-n="3"><h3>⟦essence-how⟧</h3>"#);
    if rated.is_empty() {
        b.push_str(r#"<p class="mute">⟦no-path-yet⟧</p>"#);
    }
    for (m, e) in &rated {
        let _ = write!(
            b,
            r#"<div class="serve"><div class="what"><b>{}</b><span class="chip">⟦state-{}⟧</span></div><div class="by">"#,
            esc(&m.title),
            e.state().replace(' ', "-")
        );
        let by: Vec<&&Row> = workings.iter().filter(|w| s(w, "serves") == m.id).collect();
        if by.is_empty() {
            // two kinds of gap: a step reached whose delivery nobody wrote down, and a step nothing serves
            b.push_str(if e.state() == "done" {
                r#"<p class="mute">⟦delivery-untold⟧</p>"#
            } else {
                r#"<p class="mute">⟦nothing-delivers⟧</p>"#
            });
        }
        for w in by {
            let uses = s(w, "uses");
            let link = if uses.is_empty() {
                String::new()
            } else {
                format!(
                    r#"<a class="chip" href="https://github.com/{}">{}</a>"#,
                    esc(&uses),
                    esc(uses.rsplit('/').next().unwrap_or(&uses))
                )
            };
            let _ = write!(
                b,
                r#"<div class="working"><b>{}</b><span class="chip">{}</span>{link}<p>{}</p><p class="why">{}</p></div>"#,
                esc(&s(w, "what")),
                kind_word(&s(w, "kind")),
                esc(&s(w, "how")),
                esc(&s(w, "why"))
            );
        }
        b.push_str("</div></div>");
    }
    b.push_str(r#"</section><section class="step" data-n="4"><h3>⟦essence-now⟧</h3>"#);
    b.push_str(&work);
    if !doing.is_empty() {
        b.push_str(r#"<div class="by" style="margin-top:16px">"#);
        for r in doing.iter().take(6) {
            let _ = write!(
                b,
                r#"<div class="working"><b>{}</b><span class="chip">⟦state-in-progress⟧</span><p>{}</p></div>"#,
                esc(&s(r, "title")),
                esc(&cut(&s(r, "note"), 180))
            );
        }
        b.push_str("</div>");
    }
    if work.is_empty() && doing.is_empty() {
        b.push_str(r#"<p class="mute">⟦nothing-yet⟧</p>"#);
    }
    b + "</section></div>"
}

/// A unix time as "YYYY-MM-DD HH:MM UTC" (civil-from-days, no date library).
pub fn utc(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60
    )
}

/// Renders the observatory and every grand project into `out`.
pub fn render(data: &Data, out: &Path) -> Result<()> {
    // the same site once per language, under its own folder, paths identical
    for l in LANGUAGES.iter() {
        crate::i18n::set(l.code);
        render_in(&data.localized(l.code), &out.join(l.code))?;
    }
    crate::i18n::set(DEFAULT);
    // the old addresses keep working: each page at the root leads to its default-language twin
    stubs(&out.join(DEFAULT), out, "")?;
    std::fs::write(out.join(".nojekyll"), "")?;
    Ok(())
}

/// Writes, for every page under `dir`, a redirect at the same path under `out` to the default language.
fn stubs(dir: &Path, out: &Path, rel: &str) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        let name = p
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or_default()
            .to_string();
        let r = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if p.is_dir() {
            stubs(&p, out, &r)?;
        } else if name.ends_with(".html") {
            let to = format!("{}{DEFAULT}/{r}", "../".repeat(r.matches('/').count()));
            if let Some(parent) = out.join(&r).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(out.join(&r), redirect(&to))?;
        }
    }
    Ok(())
}

pub fn redirect(to: &str) -> String {
    format!(
        r#"<!doctype html><html lang="{DEFAULT}"><head><meta charset="utf-8"><meta http-equiv="refresh" content="0; url={to}"><link rel="canonical" href="{to}"><title>Open Commons Observatory</title></head><body><a href="{to}">{to}</a></body></html>
"#
    )
}

fn render_in(data: &Data, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let generated = format!("{} {}", t("updated"), utc(now));
    let grands = grands(data);
    let page = Page {
        grands: &grands,
        generated: &generated,
    };
    std::fs::write(
        out.join("index.html"),
        page.render(
            None,
            "",
            "Observatory",
            &format!(
                "{}{}",
                family_bar(data, "index.html"),
                observatory(data, &grands)
            ),
        ),
    )?;
    std::fs::write(
        out.join("how-we-work.html"),
        page.render(None, "how-we-work.html", "How we work", &how_we_work(data)),
    )?;
    for p in subs(data) {
        for (file, title, body) in sub_pages(data, &p) {
            let body = if p.id.starts_with(HOME) {
                format!("{}{body}", family_bar(data, &file))
            } else {
                body
            };
            std::fs::write(out.join(&file), page.render(None, &file, &title, &body))?;
        }
    }
    let status_legend = legend(&[
        ("good", "⟦works⟧"),
        ("part", "⟦partly⟧"),
        ("bad", "⟦missing-or-broken⟧"),
        ("none", "⟦not-known-yet⟧"),
        ("na", "⟦not-applicable⟧"),
    ]);
    for g in &grands {
        let dir = out.join(&g.slug);
        std::fs::create_dir_all(&dir)?;
        let fam = family(data, g);
        for v in g.lens["view"].as_array().into_iter().flatten() {
            let file = format!("{}.html", v["id"].as_str().unwrap_or("view"));
            let title = v["title"].as_str().unwrap_or("").to_string();
            let body = lens_view(data, g, &fam, v, &status_legend);
            std::fs::write(
                dir.join(&file),
                page.render(Some(g), &file, &format!("{title} · {}", g.title), &body),
            )?;
        }
    }
    std::fs::write(out.join("style.css"), STYLE)?;
    std::fs::write(out.join(".nojekyll"), "")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(tables: &[(&str, &str)]) -> Data {
        Data {
            tables: tables
                .iter()
                .map(|(t, text)| {
                    (
                        t.to_string(),
                        text.lines()
                            .filter_map(|l| {
                                serde_json::from_str::<Value>(l).ok()?.as_object().cloned()
                            })
                            .collect(),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn text_is_escaped_and_names_become_paths() {
        assert_eq!(
            esc("<a href=\"x\">&</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;"
        );
        assert_eq!(slug("Linux Phones"), "linux-phones");
    }

    #[test]
    fn every_status_has_one_meaning_and_sign() {
        assert_eq!((tone("works"), sign(tone("works"))), ("good", "✓"));
        assert_eq!(tone("missing"), "bad");
        assert_eq!(tone("not-applicable"), "na");
        assert_eq!(tone("anything else"), "none");
    }

    #[test]
    fn grand_views_come_from_lenses() {
        let d = data(&[
            (
                "lenses",
                r#"{"id":"o/lens","project":"o/lens","lens":{"lens":{"title":"Linux Phones","goal":"as good"},"family":{"owners":["o"]},"view":[{"id":"index","title":"How close","kind":"path"},{"id":"features","title":"Features","kind":"matrix","table":"feature_support","rows":"features","row_key":"feature_id","columns":"platforms","column_key":"platform_id"}]}}"#,
            ),
            ("curated", r#"{"id":"o/lens","order":1}"#),
            ("projects", "{\"id\":\"o/docs\"}\n{\"id\":\"x/other\"}"),
            (
                "features",
                r#"{"id":"calls","name":"Calls","category":"phone"}"#,
            ),
            ("platforms", r#"{"id":"ios","name":"iOS"}"#),
            (
                "feature_support",
                r#"{"feature_id":"calls","platform_id":"ios","status":"works","project":"o/docs"}"#,
            ),
        ]);
        let g = grands(&d);
        assert_eq!(
            (g[0].title.as_str(), g[0].slug.as_str(), g[0].curated),
            ("Linux Phones", "linux-phones", true)
        );
        assert_eq!(
            g[0].views,
            vec![
                ("index.html".to_string(), "How close".to_string()),
                ("features.html".to_string(), "Features".to_string())
            ]
        );
        let fam = family(&d, &g[0]);
        assert_eq!(
            fam,
            vec!["o/docs".to_string()],
            "the lens's owners, not other projects"
        );
        let v = lens_view(&d, &g[0], &fam, &g[0].lens["view"][1], "");
        assert!(
            v.contains(r#"<td class="s good" title="Calls on iOS: works">✓</td>"#),
            "a matrix drawn from the lens's parameters"
        );
    }

    #[test]
    fn columns_are_ranked_best_first_from_the_cells() {
        let mut cells = BTreeMap::new();
        for (r, c, st) in [
            ("a", "linux", "works"),
            ("b", "linux", "missing"),
            ("a", "ios", "works"),
            ("b", "ios", "works"),
        ] {
            cells.insert(
                (r.to_string(), c.to_string()),
                (st.to_string(), String::new()),
            );
        }
        let ids = vec!["a".to_string(), "b".to_string()];
        let cols = ranked(
            &cells,
            &ids,
            vec![
                ("linux".into(), "Linux".into(), "linux".into()),
                ("ios".into(), "iOS".into(), "proprietary".into()),
            ],
        );
        assert_eq!(
            cols.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["ios", "linux"]
        );
        assert!((cols[1].share - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_family_shows_the_best_any_of_its_devices_reaches() {
        let d = data(&[
            ("devices", "{\"id\":\"p1\",\"soc\":\"SM7150\"}\n{\"id\":\"p2\",\"soc\":\"SM7150\"}"),
            ("support_status", "{\"component_id\":\"mic\",\"device_id\":\"p1\",\"status\":\"broken\"}\n{\"component_id\":\"mic\",\"device_id\":\"p2\",\"status\":\"works\"}"),
        ]);
        let rows: Vec<&Row> = d.rows("support_status").iter().collect();
        let f = families(&rows, d.rows("devices"));
        assert_eq!(
            f[&("mic".to_string(), "SM7150".to_string())],
            ("works".to_string(), "best on p2".to_string())
        );
    }

    #[test]
    fn a_table_has_group_rows_signs_and_unknowns() {
        let lines = vec![Line {
            id: "calls".into(),
            name: "Calls".into(),
            group: "phone".into(),
            about: String::new(),
        }];
        let cols = vec![
            Column {
                id: "ios".into(),
                name: "iOS".into(),
                share: 1.0,
                kind: String::new(),
            },
            Column {
                id: "x".into(),
                name: "X".into(),
                share: 0.0,
                kind: String::new(),
            },
        ];
        let mut cells = BTreeMap::new();
        cells.insert(
            ("calls".to_string(), "ios".to_string()),
            ("works".to_string(), String::new()),
        );
        let t = table("Feature", &lines, &cols, &cells);
        assert!(t.contains(r#"<tr class="group"><th colspan="3">phone</th></tr>"#));
        assert!(t.contains(r#"<td class="s good" title="Calls on iOS: works">✓</td>"#));
        assert!(
            t.contains(r#"title="Calls on X: unknown">?</td>"#),
            "no cell is left blank"
        );
        assert!(t.contains("<small>100%</small>"));
    }

    #[test]
    fn dates_without_a_date_library() {
        assert_eq!(utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(
            utc(1_794_322_311),
            "2026-11-10 14:51 UTC",
            "53511 s into the day"
        );
    }

    #[test]
    fn the_runs_chart_has_a_bar_per_kind_present() {
        let d = data(&[(
            "test_runs",
            "{\"run_at\":\"2026-11-10T02:13\",\"passed\":9,\"failed\":6,\"skipped\":2}",
        )]);
        let rows: Vec<&Row> = d.rows("test_runs").iter().collect();
        let c = runs_chart(&rows);
        assert_eq!(c.matches("<rect").count(), 3);
        assert!(c.contains("<title>9 passed</title>"));
    }

    #[test]
    fn unknown_is_counted_as_unknown_never_as_failure() {
        let mut cells = Cells::new();
        cells.insert(
            ("a".to_string(), "linux".to_string()),
            ("works".to_string(), String::new()),
        );
        cells.insert(
            ("b".to_string(), "linux".to_string()),
            ("missing".to_string(), String::new()),
        );
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            tally(&cells, &ids, "linux"),
            [1, 0, 1, 1],
            "c has no cell: not known yet"
        );
    }

    #[test]
    fn criteria_read_the_data_and_stars_expand() {
        let d = data(&[
            ("features", "{\"id\":\"calls\"}\n{\"id\":\"sms\"}"),
            ("feature_support", "{\"feature_id\":\"calls\",\"platform_id\":\"linux-best\",\"status\":\"works\"}"),
            ("test_runs", "{\"device_id\":\"p\",\"run_at\":\"1\",\"failed\":2}\n{\"device_id\":\"p\",\"run_at\":\"2\",\"failed\":0}"),
            ("backlog", "{\"id\":\"up\",\"status\":\"in-progress\"}"),
        ]);
        assert_eq!(
            evaluate(&d, "", "feature:*@linux-best"),
            [
                ("calls".to_string(), "works".to_string()),
                ("sms".to_string(), "unknown".to_string())
            ]
        );
        assert_eq!(
            evaluate(&d, "", "tests@p")[0].1,
            "works",
            "the latest run counts"
        );
        assert_eq!(evaluate(&d, "", "backlog:up")[0].1, "partial");
        assert_eq!(evaluate(&d, "", "backlog:gone")[0].1, "unknown");
    }

    #[test]
    fn a_milestones_state_follows_its_evidence() {
        let e = |g, p, b, u| Evidence {
            good: g,
            part: p,
            bad: b,
            unknown: u,
        };
        assert_eq!(e(3, 0, 0, 0).state(), "done");
        assert_eq!(e(1, 0, 2, 1).state(), "in progress");
        assert_eq!(e(0, 0, 2, 1).state(), "not started");
        assert_eq!(
            e(0, 0, 0, 4).state(),
            "unknown",
            "nothing measured is not 'not started'"
        );
    }

    #[test]
    fn the_path_has_a_stop_per_milestone_and_the_ladder() {
        let m = |id: &str, h: &str| Milestone {
            id: id.into(),
            subject: "x".into(),
            ord: 1,
            title: id.into(),
            horizon: h.into(),
            readiness: 6,
            criteria: vec![],
            why: String::new(),
            project: String::new(),
        };
        let e = Evidence {
            good: 1,
            part: 0,
            bad: 0,
            unknown: 0,
        };
        let svg = path_view(&[(m("a", "working"), e), (m("b", "ongoing"), e)]);
        assert_eq!(svg.matches(r#"class="stop""#).count(), 2);
        assert_eq!(svg.matches("readiness level").count(), 9);
        assert!(
            svg.contains("level 6 of 9"),
            "the highest readiness of a done milestone"
        );
    }

    #[test]
    fn readiness_is_earned_in_order() {
        let m = |id: &str, h: &str, r: i64| Milestone {
            id: id.into(),
            subject: "x".into(),
            ord: 1,
            title: id.into(),
            horizon: h.into(),
            readiness: r,
            criteria: vec![],
            why: String::new(),
            project: String::new(),
        };
        let done = Evidence {
            good: 1,
            part: 0,
            bad: 0,
            unknown: 0,
        };
        let open = Evidence {
            good: 0,
            part: 1,
            bad: 0,
            unknown: 0,
        };
        assert_eq!(
            readiness(&[
                (m("a", "working", 5), done),
                (m("b", "working", 6), open),
                (m("c", "ongoing", 8), done)
            ]),
            5,
            "c done early does not lift it"
        );
        assert_eq!(
            readiness(&[(m("a", "working", 5), open), (m("c", "ongoing", 8), done)]),
            0
        );
    }

    #[test]
    fn every_project_with_milestones_gets_its_pages() {
        let d = data(&[
            ("projects", "{\"id\":\"o/tad\",\"name\":\"tad: text as data\",\"kind\":\"tool\",\"parent\":\"x\",\"link\":\"https://x\"}\n{\"id\":\"o/docs\",\"name\":\"Phones: big\",\"kind\":\"picture\",\"parent\":\"x\"}\n{\"id\":\"o/none\",\"name\":\"no path\",\"kind\":\"tool\",\"parent\":\"x\"}"),
            ("milestones", "{\"id\":\"m\",\"subject\":\"tad\",\"ord\":1,\"title\":\"Works\",\"horizon\":\"working\",\"readiness\":6,\"criteria\":\"backlog:b\",\"why\":\"\",\"project\":\"o/tad\"}\n{\"id\":\"g\",\"subject\":\"phones\",\"ord\":1,\"title\":\"G\",\"horizon\":\"working\",\"readiness\":6,\"criteria\":\"\",\"why\":\"\",\"project\":\"o/docs\"}"),
            ("backlog", "{\"id\":\"b\",\"title\":\"B\",\"status\":\"done\",\"project\":\"o/tad\"}"),
        ]);
        let subs = subs(&d);
        assert_eq!(subs.len(), 1, "only a non-grand project with milestones");
        assert_eq!(
            (
                subs[0].title.as_str(),
                subs[0].slug.as_str(),
                subs[0].goal.as_str()
            ),
            ("tad", "project-tad", "text as data")
        );
        let pages = sub_pages(&d, &subs[0]);
        assert_eq!(
            pages.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(),
            [
                "project-tad.html",
                "project-tad-hand.html",
                "project-tad-words.html",
                "project-tad-idea.html",
                "project-tad-backlog.html",
                "project-tad-log.html",
                "project-tad-ideas.html"
            ]
        );
        assert!(
            pages[0].2.contains("<b>1 / 1</b> ⟦milestones-done⟧"),
            "its milestone is done by its backlog"
        );
        assert!(pages[0]
            .2
            .contains(r#"<a href="project-tad.html" class="on">⟦view-essence⟧</a>"#));
    }

    #[test]
    fn the_work_speaks_for_itself() {
        let now = 1_794_322_311; // 2026-11-10
        let d = data(&[
            ("activity", "{\"id\":\"2026-11-10\",\"commits\":5,\"project\":\"o/p\"}\n{\"id\":\"2026-11-09\",\"commits\":2,\"project\":\"o/p\"}\n{\"id\":\"2025-01-01\",\"commits\":9,\"project\":\"o/p\"}"),
            ("releases", "{\"id\":\"v1\",\"published\":\"2026-11-01\",\"link\":\"https://r\",\"project\":\"o/p\"}"),
            ("ci", "{\"id\":\"CI\",\"conclusion\":\"failure\",\"link\":\"https://c\",\"project\":\"o/p\"}"),
        ]);
        let w = made_by_work(&d, &["o/p".to_string()], now);
        assert!(
            w.contains("<b>7 commits in the last 90 days</b>"),
            "only the last 90 days count"
        );
        assert!(w.contains("⟦latest-release⟧ v1 · 2026-11-01"));
        assert!(w.contains(r#"class="chip ci bad""#) && w.contains("✕ CI"));
        assert_eq!(w.matches("<rect").count(), 90);
        assert!(
            made_by_work(&d, &["o/none".to_string()], now).is_empty(),
            "nothing gathered, nothing shown"
        );
    }

    #[test]
    fn every_page_offers_its_twin_in_every_language() {
        crate::i18n::set("ru");
        let grands: Vec<Grand> = vec![];
        let p = Page {
            grands: &grands,
            generated: "x",
        };
        let html = p.render(None, "how-we-work.html", "T", "B");
        assert!(html.contains(r#"<html lang="ru">"#));
        assert!(
            html.contains(r#"<b lang="ru">Русский</b>"#),
            "the current language, in itself"
        );
        assert!(
            html.contains(
                r#"<a href="../en/how-we-work.html" hreflang="en" lang="en">English</a>"#
            ),
            "the same page in English"
        );
        assert!(
            html.contains(r#"<link rel="alternate" hreflang="en" href="../en/how-we-work.html">"#)
        );
        assert!(html.contains(">Как мы работаем</a>"));
        crate::i18n::set("en");
        assert!(redirect("../en/linux-phones/index.html")
            .contains(r#"url=../en/linux-phones/index.html"#));
    }

    #[test]
    fn marks_resolve_in_the_page_language() {
        crate::i18n::set("ru");
        assert_eq!(
            localize("<h2>⟦grand-projects⟧</h2><p>⟦no-such⟧</p>"),
            "<h2>Большие проекты</h2><p>no-such</p>"
        );
        crate::i18n::set("en");
        assert_eq!(
            localize("<b>⟦rejected⟧</b> plain ⟦"),
            "<b>Rejected:</b> plain ⟦",
            "an unclosed mark stays as written"
        );
    }

    #[test]
    fn a_project_translates_its_own_data_and_only_its_own() {
        let d = data(&[
            ("lenses", r#"{"id":"o/lens","project":"o/lens","lens":{"lens":{"title":"Linux Phones"},"view":[{"id":"features","title":"Features"}]}}"#),
            ("backlog", "{\"id\":\"b\",\"title\":\"Sound\",\"project\":\"o/docs\"}\n{\"id\":\"c\",\"title\":\"Mic\",\"project\":\"o/docs\"}"),
            (
                "translations",
                "{\"table\":\"lenses\",\"row\":\"o/lens\",\"column\":\"lens.title\",\"lang\":\"ru\",\"text\":\"Linux-телефоны\",\"project\":\"o/lens\"}\n{\"table\":\"lenses\",\"row\":\"o/lens\",\"column\":\"view.features.title\",\"lang\":\"ru\",\"text\":\"Функции\",\"project\":\"o/lens\"}\n{\"table\":\"backlog\",\"row\":\"b\",\"column\":\"title\",\"lang\":\"ru\",\"text\":\"Звук\",\"project\":\"o/docs\"}\n{\"table\":\"backlog\",\"row\":\"c\",\"column\":\"title\",\"lang\":\"ru\",\"text\":\"Чужой перевод\",\"project\":\"o/lens\"}",
            ),
        ]);
        let ru = d.localized("ru");
        let g = grands(&ru);
        assert_eq!(g[0].title, "Linux-телефоны");
        assert_eq!(
            g[0].slug, "linux-phones",
            "the address is the same in every language"
        );
        assert_eq!(g[0].views[0].1, "Функции");
        assert_eq!(
            s(&ru.rows("backlog")[0], "title"),
            "Звук",
            "its own project's translation"
        );
        assert_eq!(
            s(&ru.rows("backlog")[1], "title"),
            "Mic",
            "another project may not translate it"
        );
        assert_eq!(
            grands(&d.localized("en"))[0].title,
            "Linux Phones",
            "untranslated: as written"
        );
    }

    #[test]
    fn words_are_laid_out_by_the_browser_never_placed_in_a_drawing() {
        let long = "Everything a phone does, and every other thing anyone could ask of it";
        let m = |title: &str| Milestone {
            id: "m".into(),
            subject: "x".into(),
            ord: 1,
            title: title.into(),
            horizon: "working".into(),
            readiness: 6,
            criteria: vec![],
            why: String::new(),
            project: String::new(),
        };
        let e = Evidence {
            good: 0,
            part: 1,
            bad: 0,
            unknown: 0,
        };
        let h = path_view(&[(m(long), e), (m("Calls and messages"), e)]);
        assert!(
            h.contains(&format!(r#"<span class="stitle">{long}</span>"#)),
            "the whole title, never cut"
        );
        assert!(
            !h.contains("<text"),
            "no words inside SVG: they could not wrap, and would overlap"
        );
        assert_eq!(
            h.matches(r#"class="stopcell"#).count(),
            2,
            "each stop a cell of its own"
        );
    }

    #[test]
    fn a_lens_names_its_rows_and_orders_its_groups_in_every_language() {
        let d = data(&[
            ("lenses", r#"{"id":"o/lens","project":"o/lens","lens":{"lens":{"title":"P"},"family":{"owners":["o"]},"view":[{"id":"features","title":"Features","kind":"matrix","table":"feature_support","rows":"features","row_title":"Feature","row_key":"feature_id","columns":"platforms","column_key":"platform_id","groups":["calls","power"]}]}}"#),
            ("projects", "{\"id\":\"o/docs\"}"),
            ("features", "{\"id\":\"battery\",\"name\":\"Battery\",\"category\":\"power\",\"project\":\"o/docs\"}\n{\"id\":\"voice\",\"name\":\"Voice\",\"category\":\"calls\",\"project\":\"o/docs\"}"),
            ("platforms", r#"{"id":"ios","name":"iOS"}"#),
            ("feature_support", "{\"feature_id\":\"battery\",\"platform_id\":\"ios\",\"status\":\"works\",\"project\":\"o/docs\"}\n{\"feature_id\":\"voice\",\"platform_id\":\"ios\",\"status\":\"works\",\"project\":\"o/docs\"}"),
            ("translations", "{\"table\":\"features\",\"row\":\"voice\",\"column\":\"category\",\"lang\":\"ru\",\"text\":\"звонки\",\"project\":\"o/docs\"}"),
        ]);
        let ru = d.localized("ru");
        let g = grands(&ru);
        let v = lens_view(&ru, &g[0], &family(&ru, &g[0]), &g[0].lens["view"][0], "");
        assert!(
            v.contains(r#"<th class="first">Feature</th>"#),
            "the lens names its rows"
        );
        assert!(
            v.find("звонки").unwrap() < v.find("power").unwrap(),
            "the lens's order holds, though the category is shown translated"
        );
    }

    #[test]
    fn pale_cells_keep_dark_signs() {
        // the table's "cell text is white" rule (.grid td.s) is more specific than a bare .s.none:
        // the pale cells' rule must be at least as specific, or their signs turn white on pale
        assert!(STYLE.contains(".grid td.s.none{") && STYLE.contains(".grid td.s.na{"));
        assert!(
            !STYLE.contains("}.s.none{"),
            "a bare .s.none loses to .grid td.s"
        );
    }

    #[test]
    fn a_lens_of_ideas_shows_its_ideas_and_no_empty_comparisons() {
        let d = data(&[
            ("lenses", r#"{"id":"m/lens","project":"m/lens","lens":{"lens":{"title":"Mnemonic Vocabulary"},"family":{"owners":["m"]},"view":[{"id":"index","title":"How close","kind":"path"},{"id":"ideas","title":"Ideas","kind":"ideas"}]}}"#),
            ("projects", "{\"id\":\"m/home\"}"),
            ("concepts", "{\"id\":\"k\",\"name\":\"The memory kingdom\",\"summary\":\"Reflected.\",\"why\":\"So it is kept.\",\"project\":\"m/home\"}"),
            ("milestones", "{\"id\":\"w\",\"subject\":\"mnemonic-vocabulary\",\"ord\":1,\"title\":\"Written\",\"horizon\":\"working\",\"readiness\":1,\"criteria\":\"backlog:i\",\"why\":\"\",\"project\":\"m/home\"}"),
            ("backlog", "{\"id\":\"i\",\"status\":\"done\",\"project\":\"m/home\"}"),
        ]);
        let g = grands(&d);
        let fam = family(&d, &g[0]);
        let ideas = lens_view(&d, &g[0], &fam, &g[0].lens["view"][1], "");
        assert!(ideas.contains("The memory kingdom") && ideas.contains("So it is kept."));
        let how = lens_view(&d, &g[0], &fam, &g[0].lens["view"][0], "");
        assert!(how.contains("Written"), "its path");
        assert!(
            !how.contains("every-candidate"),
            "no comparison of candidates when there are none"
        );
    }

    #[test]
    fn not_yet_is_not_failure() {
        let d = data(&[("backlog", "{\"id\":\"later\",\"status\":\"open\"}")]);
        let items = evaluate(&d, "", "backlog:later");
        assert_eq!(items[0].1, "open");
        assert_eq!(
            (tone("open"), sign(tone("open"))),
            ("todo", "○"),
            "planned work has its own calm sign, never the red one"
        );
        let e = evidence(&items);
        assert_eq!(e.state(), "not started");
        let m = Milestone {
            id: "m".into(),
            subject: "x".into(),
            ord: 1,
            title: "Later".into(),
            horizon: "working".into(),
            readiness: 5,
            criteria: vec![],
            why: String::new(),
            project: String::new(),
        };
        let h = path_view(&[(m, e)]);
        assert!(h.contains(r#"class="ring todo""#) && !h.contains(r#"class="ring bad""#));
    }

    #[test]
    fn a_criterion_is_read_in_its_own_project_first_then_anywhere() {
        let d = data(&[("backlog", "{\"id\":\"site\",\"status\":\"open\",\"project\":\"a/one\"}\n{\"id\":\"site\",\"status\":\"done\",\"project\":\"b/two\"}\n{\"id\":\"rhizcut\",\"status\":\"in-progress\",\"project\":\"c/eco\"}")]);
        assert_eq!(
            evaluate(&d, "b/two", "backlog:site")[0].1,
            "works",
            "its own project's item, not the first one found"
        );
        assert_eq!(evaluate(&d, "a/one", "backlog:site")[0].1, "open");
        assert_eq!(
            evaluate(&d, "a/one", "backlog:rhizcut")[0].1,
            "partial",
            "another project's work, when its own has none: chaining"
        );
    }

    #[test]
    fn lenses_not_curated_are_shown_as_discovered() {
        let d = data(&[
            ("lenses", "{\"id\":\"o/lens\",\"project\":\"o/lens\",\"lens\":{\"lens\":{\"title\":\"Ours\"},\"view\":[]}}\n{\"id\":\"x/lens\",\"project\":\"x/lens\",\"lens\":{\"lens\":{\"title\":\"Theirs\"},\"view\":[]}}"),
            ("curated", "{\"id\":\"o/lens\",\"order\":1}"),
        ]);
        let g = grands(&d);
        let page = observatory(&d, &g);
        let (ours, heading, theirs) = (
            page.find("Ours").unwrap(),
            page.find("discovered-lenses").unwrap(),
            page.find("Theirs").unwrap(),
        );
        assert!(
            ours < heading && heading < theirs,
            "curated first, then the discovered ones under their own heading"
        );
    }

    #[test]
    fn an_idea_reads_as_an_article_in_its_own_order() {
        let d = data(&[("idea", "{\"id\":\"b\",\"ord\":2,\"kind\":\"picture\",\"heading\":\"Like a cutting\",\"text\":\"It grows in your soil.\",\"project\":\"o/r\"}\n{\"id\":\"a\",\"ord\":1,\"kind\":\"lead\",\"heading\":\"\",\"text\":\"You do not **own** it.\\n\\nSecond paragraph <here>.\",\"project\":\"o/r\"}\n{\"id\":\"c\",\"ord\":3,\"kind\":\"limit\",\"heading\":\"What is still hard\",\"text\":\"Much.\",\"project\":\"o/r\"}")]);
        let rows: Vec<&Row> = d.rows("idea").iter().collect();
        let h = idea_html(&rows);
        assert!(
            h.find("You do not").unwrap() < h.find("Like a cutting").unwrap(),
            "in the order it was written, not the order of ids"
        );
        assert!(
            h.contains("<p>You do not <b>own</b> it.</p><p>Second paragraph &lt;here&gt;.</p>"),
            "paragraphs and bold, made safe"
        );
        assert!(
            h.contains("<blockquote><b>Like a cutting</b>")
                && h.contains(r#"<div class="limit"><h3>What is still hard</h3>"#)
        );
        assert!(
            idea_html(&[]).contains("idea-not-written"),
            "a project that has not written its idea says so"
        );
    }

    #[test]
    fn essence_shows_the_whole_chain_and_its_gaps() {
        let d = data(&[
            ("idea", "{\"id\":\"1\",\"ord\":1,\"kind\":\"lead\",\"heading\":\"\",\"text\":\"Why it is.\\n\\nMore.\",\"project\":\"o/p\"}"),
            ("milestones", "{\"id\":\"m1\",\"subject\":\"p\",\"ord\":1,\"title\":\"Written\",\"horizon\":\"working\",\"readiness\":3,\"criteria\":\"backlog:a\",\"why\":\"\",\"project\":\"o/p\"}\n{\"id\":\"m2\",\"subject\":\"p\",\"ord\":2,\"title\":\"Everywhere\",\"horizon\":\"ongoing\",\"readiness\":9,\"criteria\":\"backlog:b\",\"why\":\"\",\"project\":\"o/p\"}"),
            ("workings", "{\"id\":\"w\",\"what\":\"The guide\",\"kind\":\"practice\",\"serves\":\"m1\",\"how\":\"Says how.\",\"why\":\"So it can be learned.\",\"uses\":\"o/tool\",\"project\":\"o/p\"}"),
            ("backlog", "{\"id\":\"a\",\"status\":\"done\",\"project\":\"o/p\"}\n{\"id\":\"b\",\"title\":\"Spread it\",\"status\":\"in-progress\",\"note\":\"n\",\"project\":\"o/p\"}"),
        ]);
        let fam = vec!["o/p".to_string()];
        let ms = milestones_of(&of(&d, "milestones", &fam));
        let e = essence_html(&d, &fam, "o/p", &ms, "P", "a goal");
        for step in 1..=4 {
            assert!(
                e.contains(&format!(r#"data-n="{step}""#)),
                "step {step} of the chain"
            );
        }
        assert!(
            e.contains("<p>Why it is.</p>") && !e.contains("More."),
            "the opening of its own idea, not all of it"
        );
        assert!(
            e.contains("The guide")
                && e.contains("So it can be learned.")
                && e.contains("https://github.com/o/tool")
        );
        assert_eq!(
            e.matches("nothing-delivers").count(),
            1,
            "the step nothing serves says so"
        );
        assert!(
            e.contains(r#"<span class="tell off">○ ⟦essence-how⟧</span>"#),
            "not every step is served: the mark is off"
        );
        assert!(
            e.contains(r#"<span class="tell on">✓ ⟦essence-why⟧</span>"#)
                && e.contains("Spread it")
        );
    }

    #[test]
    fn a_step_reached_with_no_workings_is_untold_not_a_wish() {
        let d = data(&[
            ("milestones", "{\"id\":\"m1\",\"subject\":\"p\",\"ord\":1,\"title\":\"Reached\",\"horizon\":\"working\",\"readiness\":3,\"criteria\":\"backlog:a\",\"why\":\"\",\"project\":\"o/p\"}\n{\"id\":\"m2\",\"subject\":\"p\",\"ord\":2,\"title\":\"Ahead\",\"horizon\":\"working\",\"readiness\":5,\"criteria\":\"backlog:b\",\"why\":\"\",\"project\":\"o/p\"}"),
            ("backlog", "{\"id\":\"a\",\"status\":\"done\",\"project\":\"o/p\"}\n{\"id\":\"b\",\"status\":\"open\",\"project\":\"o/p\"}"),
        ]);
        let fam = vec!["o/p".to_string()];
        let e = essence_html(
            &d,
            &fam,
            "o/p",
            &milestones_of(&of(&d, "milestones", &fam)),
            "P",
            "",
        );
        assert_eq!(
            e.matches("delivery-untold").count(),
            1,
            "reached, but how was never written"
        );
        assert_eq!(
            e.matches("nothing-delivers").count(),
            1,
            "not reached, and nothing serves it"
        );
    }

    #[test]
    fn the_four_marks_follow_one_rule() {
        let d = data(&[
            ("idea", "{\"id\":\"1\",\"ord\":1,\"kind\":\"lead\",\"text\":\"Why.\",\"project\":\"o/p\"}"),
            ("milestones", "{\"id\":\"m1\",\"subject\":\"p\",\"ord\":1,\"title\":\"A\",\"horizon\":\"working\",\"readiness\":3,\"criteria\":\"backlog:a\",\"why\":\"\",\"project\":\"o/p\"}\n{\"id\":\"m2\",\"subject\":\"p\",\"ord\":2,\"title\":\"B\",\"horizon\":\"working\",\"readiness\":5,\"criteria\":\"backlog:b\",\"why\":\"\",\"project\":\"o/p\"}"),
            ("workings", "{\"id\":\"w\",\"what\":\"T\",\"kind\":\"tool\",\"serves\":\"m1\",\"how\":\"\",\"why\":\"\",\"uses\":\"\",\"project\":\"o/p\"}"),
            ("backlog", "{\"id\":\"a\",\"status\":\"done\",\"project\":\"o/p\"}"),
        ]);
        let fam = vec!["o/p".to_string()];
        let ms = milestones_of(&of(&d, "milestones", &fam));
        assert_eq!(
            told(&d, &fam, "o/p", &ms),
            [true, true, false, true],
            "one step has nothing serving it"
        );
        assert_eq!(told(&d, &fam, "o/p", &ms[..1]), [true, true, true, true]);
        assert_eq!(
            told(&d, &["o/other".to_string()], "o/other", &[]),
            [false, false, false, false],
            "a project that tells nothing"
        );
        let small = tells([true, false, false, true]);
        assert_eq!(small.matches(r#"class="on""#).count(), 2);
        assert!(small.contains("○ ⟦tell-how⟧"));
    }

    #[test]
    fn a_grand_card_counts_its_projects_once_and_in_words_from_the_catalog() {
        let d = data(&[(
            "projects",
            "{\"id\":\"o/p\",\"name\":\"P\",\"kind\":\"tool\",\"parent\":\"x\"}",
        )]);
        let g = Grand {
            id: "g".into(),
            title: "g".into(),
            goal: "g".into(),
            slug: "g".into(),
            owner: "g".into(),
            owners: vec![],
            members: vec!["o/p".into()],
            curated: true,
            order: 0,
            views: vec![],
            lens: Value::Null,
        };
        let card = grand_overview_card(&d, &g);
        assert!(
            card.contains("<p>1 project</p>"),
            "counted once, in the form its number asks for: {card}"
        );
        assert!(
            !card.contains(" projects<"),
            "no English word outside the catalog: {card}"
        );
    }

    #[test]
    fn a_working_says_its_kind_in_the_readers_language() {
        assert_eq!(kind_word("check"), "⟦working-check⟧");
        assert_eq!(
            kind_word("<odd>"),
            "&lt;odd&gt;",
            "an unknown kind is shown as written, made safe"
        );
        crate::i18n::set("ru");
        assert_eq!(localize(&kind_word("gatherer")), "сборщик");
        crate::i18n::set("en");
    }

    #[test]
    fn a_projects_own_words_are_marked_once_outside_tags() {
        let w = vec![("hypha".to_string(), "the first filament".to_string())];
        let h = mark_words(
            r#"<p>Hypha grows. <a href="hypha.html">x</a> a hypha again</p>"#,
            &w,
        );
        assert_eq!(h.matches("<abbr").count(), 1, "{h}");
        assert!(
            h.contains(r#"<abbr class="word" title="the first filament">Hypha</abbr> grows"#),
            "first use, its own case: {h}"
        );
        assert!(h.contains(r#"href="hypha.html""#), "never inside a tag");
        assert_eq!(
            mark_words("<p>hyphae</p>", &w),
            "<p>hyphae</p>",
            "whole words only"
        );
    }

    #[test]
    fn the_vocabulary_shows_own_words_then_the_shared_ones() {
        let d = data(&[("vocabulary", "{\"id\":\"lens\",\"term\":\"lens\",\"means\":\"a grand view\",\"not\":\"a page\",\"see\":\"mark\",\"project\":\"open-commons-observatory/ocoeco\"}\n{\"id\":\"shoot\",\"term\":\"shoot\",\"means\":\"what grows\",\"project\":\"o/p\"}")]);
        let h = vocabulary_html(&d, &["o/p".to_string()]);
        assert!(h.find("words-own").unwrap() < h.find("words-shared").unwrap());
        assert!(
            h.contains(r#"id="w-shoot""#)
                && h.contains(r#"id="w-lens""#)
                && h.contains(r##"<a href="#w-mark">mark</a>"##)
        );
    }

    #[test]
    fn attention_shows_what_waits_what_nothing_serves_and_what_went_quiet() {
        let d = data(&[
            ("projects", "{\"id\":\"o/p\",\"name\":\"P\",\"kind\":\"tool\",\"parent\":\"x\"}"),
            ("backlog", "{\"id\":\"sim\",\"title\":\"A SIM card\",\"status\":\"open\",\"waits\":\"author\",\"project\":\"o/p\"}\n{\"id\":\"w\",\"title\":\"W\",\"status\":\"in-progress\",\"project\":\"o/p\"}"),
            ("milestones", "{\"id\":\"m1\",\"subject\":\"p\",\"ord\":1,\"title\":\"Calls\",\"horizon\":\"working\",\"readiness\":3,\"criteria\":\"backlog:w\",\"why\":\"\",\"project\":\"o/p\"}"),
            ("activity", "{\"id\":\"2020-01-01\",\"commits\":3,\"project\":\"o/p\"}"),
        ]);
        let h = attention_html(&d);
        assert!(
            h.contains("A SIM card") && h.contains(">author<"),
            "waits on a person"
        );
        assert!(h.contains("1: Calls"), "a step nothing delivers");
        assert!(
            h.contains("2020-01-01"),
            "work gone quiet since its last commit"
        );
        let parked = data(&[
            ("projects", "{\"id\":\"o/q\",\"name\":\"Q\",\"kind\":\"tool\",\"parent\":\"x\"}"),
            ("backlog", "{\"id\":\"x\",\"title\":\"X\",\"status\":\"in-progress\",\"project\":\"o/q\"}\n{\"id\":\"y\",\"title\":\"Y\",\"status\":\"open\",\"project\":\"o/r\"}"),
            ("milestones", "{\"id\":\"m\",\"subject\":\"r\",\"ord\":1,\"title\":\"Far\",\"horizon\":\"working\",\"readiness\":3,\"criteria\":\"backlog:y\",\"why\":\"\",\"project\":\"o/r\"}"),
        ]);
        let p = attention_html(&parked);
        assert!(
            !p.contains("Far"),
            "a parked project's steps wait by choice"
        );
        assert!(
            !p.contains("last commit"),
            "not gathered yet is unknown, not quiet"
        );
    }

    #[test]
    fn hand_work_shows_what_is_saturated_and_what_a_tool_replaced() {
        let d = data(&[
            ("projects", "{\"id\":\"o/p\",\"name\":\"P\",\"kind\":\"tool\",\"parent\":\"x\"}\n{\"id\":\"o/q\",\"name\":\"Q\",\"kind\":\"tool\",\"parent\":\"x\"}"),
            ("handwork", "{\"id\":\"patch\",\"action\":\"patch source by text\",\"count\":4,\"status\":\"manual\",\"minutes\":3,\"project\":\"o/p\"}\n{\"id\":\"patch\",\"action\":\"patch source by text\",\"count\":2,\"status\":\"manual\",\"project\":\"o/q\"}\n{\"id\":\"rows\",\"action\":\"write rows by hand\",\"count\":24,\"status\":\"automated\",\"automated_by\":\"ocoeco note\",\"project\":\"o/p\"}\n{\"id\":\"look\",\"action\":\"look at a page\",\"count\":9,\"status\":\"deliberate\",\"project\":\"o/p\"}"),
        ]);
        let h = handwork_html(&d, &["o/p".to_string()]);
        assert!(
            h.find("write rows by hand").unwrap() < h.find("patch source by text").unwrap(),
            "by count"
        );
        assert_eq!(
            h.matches("hand-worth").count(),
            1,
            "only the manual one past three asks for a tool"
        );
        assert!(h.contains("· ocoeco note") && h.contains("hand-status-deliberate"));
        let a = attention_html(&d);
        assert!(
            a.contains("patch source by text · 6×"),
            "summed across projects: {a}"
        );
        assert!(
            !a.contains("look at a page") && !a.contains("write rows by hand"),
            "deliberate and automated never ask"
        );
    }
}
