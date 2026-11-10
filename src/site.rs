//! The observatory's face: a designed static website from the harvested data (tad: text as data) -
//! every word and number from the data, no JavaScript. The observatory is the meta view; each grand
//! project (a marked project of kind `picture`) has its own tab and its own views: how close it is,
//! its tables, its tests, backlog and landscape. Columns are ranked from the data, best first.

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
        _ => "none",
    }
}

fn sign(t: &str) -> &'static str {
    match t {
        "good" => "✓",
        "part" => "◐",
        "bad" => "✕",
        "na" => "–",
        _ => "·",
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
}

/// The grand projects, from the registry: the title is the mark's `title`, or its name before a colon.
pub fn grands(data: &Data) -> Vec<Grand> {
    data.rows("projects")
        .iter()
        .filter(|p| s(p, "kind") == "picture")
        .map(|p| {
            let name = s(p, "name");
            let (before, after) = name
                .split_once(':')
                .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
                .unwrap_or((name.clone(), String::new()));
            let title = Some(s(p, "title"))
                .filter(|t| !t.is_empty())
                .unwrap_or(before);
            let id = s(p, "id");
            Grand {
                slug: slug(&title),
                owner: id.split('/').next().unwrap_or_default().to_string(),
                goal: after,
                title,
                id,
            }
        })
        .collect()
}

/// The projects of a grand project's family: itself, and those whose parent is its owner or itself.
fn family(data: &Data, g: &Grand) -> Vec<String> {
    data.rows("projects")
        .iter()
        .filter(|p| {
            let par = s(p, "parent");
            s(p, "id") == g.id || par == g.owner || par == g.id
        })
        .map(|p| s(p, "id"))
        .collect()
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
.s.bad{background:color-mix(in srgb,var(--bad) 85%,transparent)}.s.none{background:var(--soft);color:var(--mute)}.s.na{background:var(--na);color:var(--mute)}
.legend{display:flex;gap:18px;flex-wrap:wrap;color:var(--mute);font-size:14px;margin:18px 0 0}.legend i{display:inline-block;width:20px;height:20px;line-height:20px;text-align:center;border-radius:5px;margin-right:6px;font-style:normal;font-weight:700;color:#fff;font-size:12px}
.legend .none,.legend .na{color:var(--mute)}.legend .good{background:var(--good)}.legend .part{background:var(--part)}.legend .bad{background:var(--bad)}.legend .none{background:var(--soft)}.legend .na{background:var(--na)}
.bars{margin-top:24px}.bar{display:grid;grid-template-columns:220px 1fr 56px;gap:14px;align-items:center;margin:10px 0}
.bar span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.stack{display:flex;height:14px;border-radius:99px;overflow:hidden;background:var(--soft)}.stack i{display:block;height:100%}
.stack .good{background:var(--good)}.stack .part{background:var(--part)}.stack .bad{background:var(--bad)}.stack .none{background:var(--none)}.bar b{text-align:right;font-weight:600}
.board{display:grid;grid-template-columns:repeat(3,1fr);gap:20px}@media(max-width:820px){.board{grid-template-columns:1fr}.bar{grid-template-columns:120px 1fr 48px}}
.col .card{margin-bottom:12px}
.pathbox{background:var(--card);border:1px solid var(--line);border-radius:18px;padding:14px 10px}
svg.path .band{fill:var(--soft)}svg.path .hlabel{font-size:13px;fill:var(--mute);font-weight:600;letter-spacing:.04em}
svg.path .road{stroke:var(--line);stroke-width:6;stroke-linecap:round}svg.path .travelled{stroke:var(--good);stroke-width:6;stroke-linecap:round}
svg.path .stop{fill:var(--card);stroke:var(--line);stroke-width:5}svg.path .ring{fill:none;stroke-width:5;stroke-linecap:round}
svg.path .ring.good{stroke:var(--good)}svg.path .ring.part{stroke:var(--part)}svg.path .ring.bad{stroke:var(--bad)}svg.path .ring.none{stroke:var(--none)}
svg.path .frac{font-size:10.5px;text-anchor:middle;fill:var(--fg);font-weight:700}svg.path .stitle{font-size:12.5px;text-anchor:middle;fill:var(--fg)}
svg.path .lvl{fill:var(--soft);stroke:var(--line)}svg.path .lvl.on{fill:var(--accent);stroke:var(--accent)}
.signs{display:flex;flex-wrap:wrap;gap:3px}.mini{display:inline-block;width:18px;height:18px;line-height:18px;text-align:center;border-radius:4px;font-size:11px;font-weight:700;color:#fff}
.mini.good{background:var(--good)}.mini.part{background:var(--part)}.mini.bad{background:var(--bad)}.mini.none{background:var(--soft);color:var(--mute)}.mini.na{background:var(--na);color:var(--mute)}
footer{border-top:1px solid var(--line);color:var(--mute);font-size:14px;padding:28px 0}
"#;

const VIEWS: [(&str, &str); 7] = [
    ("index.html", "How close we are"),
    ("features.html", "Features"),
    ("devices.html", "Devices"),
    ("families.html", "SoC families"),
    ("tests.html", "Tests"),
    ("backlog.html", "Backlog"),
    ("landscape.html", "Landscape"),
];

struct Page<'a> {
    grands: &'a [Grand],
    generated: &'a str,
}

impl Page<'_> {
    /// A page: the observatory's tabs (itself, then each grand project), a grand project's own views.
    fn render(&self, grand: Option<&Grand>, view: &str, title: &str, body: &str) -> String {
        let up = if grand.is_some() { "../" } else { "" };
        let mut nav = format!(
            r#"<a href="{up}index.html"{}>Observatory</a>"#,
            if grand.is_none() {
                r#" class="on""#
            } else {
                ""
            }
        );
        for g in self.grands {
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
                for (f, t) in VIEWS {
                    let _ = write!(
                        x,
                        r#"<a href="{f}"{}>{t}</a>"#,
                        if f == view { r#" class="on""# } else { "" }
                    );
                }
                x + "</div></div>"
            })
            .unwrap_or_default();
        format!(
            r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{t} · Open Commons Observatory</title><link rel="stylesheet" href="{up}style.css"></head><body>
<header><div class="wrap"><a class="mark" href="{up}index.html">Open Commons Observatory</a><nav>{nav}</nav></div></header>{sub}
<main><div class="wrap">{body}</div></main>
<footer><div class="wrap">Every word and number here comes from the projects' own data, gathered by the observatory · {g}</div></footer>
</body></html>
"#,
            t = esc(title),
            g = esc(self.generated)
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
        b.push_str(&path_section(data, "The path", &mine));
    }
    if let Some(l) = cols.iter().find(|c| c.kind == "linux") {
        let [w, p, m, u] = tally(&cells, &ids, &l.id);
        let _ = write!(
            b,
            r#"<div class="stats"><div class="stat"><b>{w}</b><span>features known to work on {}</span></div><div class="stat"><b>{p}</b><span>partly</span></div><div class="stat"><b>{m}</b><span>missing</span></div><div class="stat"><b>{u}</b><span>not known yet - to measure</span></div></div>"#,
            esc(&l.name)
        );
    }
    b.push_str(r#"<h2>Every candidate, feature by feature</h2>"#);
    b.push_str(&legend(&[
        ("good", "works"),
        ("part", "partly"),
        ("bad", "missing"),
        ("none", "not known yet"),
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
    b.push_str("</div><h2>By area</h2>");
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
            format!("{w} of {} features known to work on {}, {p} partly, {m} missing, {u} still to measure", ids.len(), l.name)
        }
        None => format!("{} projects", fam.len()),
    };
    let ms: Vec<Milestone> = milestones_of(&of(data, "milestones", &fam))
        .into_iter()
        .filter(|m| m.subject == g.slug)
        .collect();
    let rated: Vec<(Milestone, Evidence)> = ms
        .iter()
        .map(|m| {
            let items: Vec<(String, String)> =
                m.criteria.iter().flat_map(|c| evaluate(data, c)).collect();
            (m.clone(), evidence(&items))
        })
        .collect();
    let path = if rated.is_empty() {
        String::new()
    } else {
        path_svg(&rated)
    };
    format!(
        r#"<a class="grand" href="{}/index.html"><h3>{}</h3><p>{}</p>{path}<p>{} · {} projects</p></a>"#,
        g.slug,
        esc(&g.title),
        esc(&g.goal),
        esc(&line),
        fam.len()
    )
}

fn observatory(data: &Data, grands: &[Grand]) -> String {
    let projects = data.rows("projects");
    let open = data
        .rows("backlog")
        .iter()
        .filter(|r| s(r, "status") != "done")
        .count();
    let mut b = format!(
        r#"<h1>Open Commons Observatory</h1><p class="lead">One living picture of the open commons. Every project keeps its own state as data; the observatory finds the marked ones and gathers what they share - nothing is wired in by hand.</p>
<div class="stats"><div class="stat"><b>{}</b><span>grand projects</span></div><div class="stat"><b>{}</b><span>projects in the picture</span></div><div class="stat"><b>{open}</b><span>open items, all backlogs</span></div><div class="stat"><b>{}</b><span>session log entries</span></div></div><h2>Grand projects</h2>"#,
        grands.len(),
        projects.len(),
        data.rows("session_log").len()
    );
    for g in grands {
        b.push_str(&grand_overview_card(data, g));
    }
    // the other projects with a path of their own - the same view, below the grand projects
    let others: Vec<&Row> = projects
        .iter()
        .filter(|p| {
            s(p, "kind") != "picture"
                && data
                    .rows("milestones")
                    .iter()
                    .any(|m| s(m, "project") == s(p, "id"))
        })
        .collect();
    if !others.is_empty() {
        b.push_str(r#"<h2>Other projects on their way</h2>"#);
        for p in others {
            let rows: Vec<&Row> = data
                .rows("milestones")
                .iter()
                .filter(|m| s(m, "project") == s(p, "id"))
                .collect();
            let name = s(p, "name");
            let title = Some(s(p, "title"))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| name.split(':').next().unwrap_or(&name).trim().to_string());
            b.push_str(&path_section(data, &title, &milestones_of(&rows)));
        }
    }
    let ids: Vec<String> = projects.iter().map(|p| s(p, "id")).collect();
    let mut parents: Vec<String> = Vec::new();
    for p in projects {
        if !parents.contains(&s(p, "parent")) {
            parents.push(s(p, "parent"));
        }
    }
    b.push_str("<h2>Every project</h2>");
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
                r#"<div class="card"><h4><a href="{}">{}</a></h4><span class="chip">{}</span><span class="chip">{tables} shared tables</span><p>{}</p></div>"#,
                esc(&s(p, "link")),
                esc(&s(p, "name")),
                esc(&s(p, "kind")),
                esc(&s(p, "id"))
            );
        }
        b.push_str("</div>");
    }
    let mut log: Vec<&Row> = data.rows("session_log").iter().collect();
    log.sort_by_key(|r| std::cmp::Reverse(s(r, "entry_date")));
    b.push_str("<h2>Latest work</h2>");
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
        r#"<h1>Backlog</h1><p class="lead">The work ahead and done.</p><div class="board" style="margin-top:32px">"#,
    );
    for (st, title) in [
        ("open", "Open"),
        ("in-progress", "In progress"),
        ("done", "Done"),
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

fn landscape(rows: &[&Row]) -> String {
    let mut b = String::from(
        r#"<h1>Landscape</h1><p class="lead">The projects around ours - what each does, and how we build on it rather than beside it.</p><div class="cards" style="margin-top:32px">"#,
    );
    for r in rows {
        let _ = write!(
            b,
            r#"<div class="card"><h4><a href="{}">{}</a></h4><span class="chip">{}</span><p>{}</p><p><b>For us:</b> {}</p></div>"#,
            esc(&s(r, "link")),
            esc(&s(r, "name")),
            esc(&s(r, "project")),
            esc(&s(r, "what")),
            esc(&s(r, "for_us"))
        );
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
        })
        .collect();
    ms.sort_by_key(|m| (m.subject.clone(), m.ord));
    ms
}

/// One criterion, read from the data: (what, status) for each thing it names - `feature:ID@PLATFORM`,
/// `component:ID@DEVICE` (`*` for every one), `tests@DEVICE` (the latest run), `backlog:ID`.
pub fn evaluate(data: &Data, criterion: &str) -> Vec<(String, String)> {
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
            .find(|r| s(r, "id") == id)
            .map(|r| s(r, "status"))
        {
            Some(x) if x == "done" => "works",
            Some(x) if x == "in-progress" => "partial",
            Some(_) => "missing",
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
            "bad" => e.bad += 1,
            "na" => {}
            _ => e.unknown += 1,
        }
    }
    e
}

const HORIZONS: [(&str, &str); 3] = [
    ("working", "Working"),
    ("perfection", "Perfection"),
    ("ongoing", "Ongoing perfection"),
];

/// The path: three horizons, a stop per milestone with its evidence, the way travelled so far, and
/// the readiness ladder.
pub fn path_svg(ms: &[(Milestone, Evidence)]) -> String {
    let (w, top, line_y) = (1000.0_f64, 34.0_f64, 96.0_f64);
    let total = ms.len().max(1) as f64;
    let mut svg = format!(
        r#"<svg class="path" viewBox="0 0 {w} 210" width="100%" role="img" aria-label="the path">"#
    );
    let mut x0 = 20.0;
    let span = w - 40.0;
    let mut stops: Vec<(f64, &Milestone, &Evidence)> = Vec::new();
    for (h, label) in HORIZONS {
        let in_h: Vec<&(Milestone, Evidence)> = ms.iter().filter(|(m, _)| m.horizon == h).collect();
        let bw = span * (in_h.len().max(1) as f64 / total.max(3.0)).max(0.12);
        let _ = write!(
            svg,
            r#"<rect x="{x0:.1}" y="{top}" width="{:.1}" height="124" rx="14" class="band"/><text x="{:.1}" y="{}" class="hlabel">{label}</text>"#,
            bw - 8.0,
            x0 + 14.0,
            top + 22.0
        );
        for (i, (m, e)) in in_h.iter().enumerate() {
            stops.push((x0 + bw * (i as f64 + 0.5) / in_h.len() as f64, m, e));
        }
        x0 += bw;
    }
    // the road is travelled only as far as every stop before it is done
    let run = stops
        .iter()
        .take_while(|(_, _, e)| e.state() == "done")
        .count();
    let reached = if run == 0 { 20.0 } else { stops[run - 1].0 };
    let _ = write!(
        svg,
        r#"<line x1="20" y1="{line_y}" x2="{:.1}" y2="{line_y}" class="road"/><line x1="20" y1="{line_y}" x2="{reached:.1}" y2="{line_y}" class="travelled"/>"#,
        x0 - 8.0
    );
    for (x, m, e) in &stops {
        let r = 15.0;
        let c = 2.0 * std::f64::consts::PI * r;
        let tn = match e.state() {
            "done" => "good",
            "in progress" => "part",
            "not started" => "bad",
            _ => "none",
        };
        let _ = write!(
            svg,
            r#"<g><title>{}: {} - {} of {} criteria good, {} partly, {} not yet, {} unknown</title><circle cx="{x:.1}" cy="{line_y}" r="{r}" class="stop"/><circle cx="{x:.1}" cy="{line_y}" r="{r}" class="ring {tn}" stroke-dasharray="{:.1} {c:.1}" transform="rotate(-90 {x:.1} {line_y})"/><text x="{x:.1}" y="{}" class="frac">{}/{}</text><text x="{x:.1}" y="{}" class="stitle">{}</text></g>"#,
            esc(&m.title),
            e.state(),
            e.good,
            e.total(),
            e.part,
            e.bad,
            e.unknown,
            c * e.share(),
            line_y + 4.5,
            e.good,
            e.total(),
            line_y + 40.0,
            esc(&cut(&m.title, 22))
        );
    }
    // readiness is earned in order: the highest level in the unbroken run of done milestones
    let level = readiness(ms);
    let _ = write!(
        svg,
        r#"<text x="20" y="196" class="hlabel">Readiness</text>"#
    );
    for l in 1..=9 {
        let _ = write!(
            svg,
            r#"<rect x="{}" y="186" width="26" height="10" rx="3" class="{}"><title>readiness level {l}</title></rect>"#,
            112 + (l - 1) * 30,
            if l <= level { "lvl on" } else { "lvl" }
        );
    }
    let _ = write!(
        svg,
        r#"<text x="{}" y="196" class="hlabel">level {level} of 9</text>"#,
        112 + 9 * 30 + 6
    );
    svg + "</svg>"
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
            let items: Vec<(String, String)> =
                m.criteria.iter().flat_map(|c| evaluate(data, c)).collect();
            (m.clone(), evidence(&items))
        })
        .collect();
    let mut b = format!(
        r#"<h2>{}</h2><div class="pathbox">{}</div><div class="cards" style="margin-top:16px">"#,
        esc(title),
        path_svg(&rated)
    );
    for (m, e) in &rated {
        let items: Vec<(String, String)> =
            m.criteria.iter().flat_map(|c| evaluate(data, c)).collect();
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
            r#"<div class="card" id="{}"><h4>{}</h4><span class="chip">{h}</span><span class="chip">readiness {}</span><span class="chip">{}</span><p>{}</p><p class="signs">{signs}</p></div>"#,
            esc(&m.id),
            esc(&m.title),
            m.readiness,
            e.state(),
            esc(&m.why)
        );
    }
    b + "</div>"
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
    std::fs::create_dir_all(out)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let generated = format!("updated {}", utc(now));
    let grands = grands(data);
    let page = Page {
        grands: &grands,
        generated: &generated,
    };
    std::fs::write(
        out.join("index.html"),
        page.render(None, "", "Observatory", &observatory(data, &grands)),
    )?;
    let status_legend = legend(&[
        ("good", "works"),
        ("part", "partly"),
        ("bad", "missing or broken"),
        ("none", "not known yet"),
        ("na", "not applicable"),
    ]);
    for g in &grands {
        let dir = out.join(&g.slug);
        std::fs::create_dir_all(&dir)?;
        let fam = family(data, g);
        let (lines, cols, cells) = feature_view(data, &fam);
        let support = of(data, "support_status", &fam);
        let dcells = cells_of(&support, "component_id", "device_id");
        let dlines = lines_from(
            data,
            "components",
            |id| dcells.keys().any(|(r, _)| r == id),
            "notes",
        );
        let dids: Vec<String> = dlines.iter().map(|l| l.id.clone()).collect();
        let dcols = ranked(
            &dcells,
            &dids,
            data.rows("devices")
                .iter()
                .map(|d| (s(d, "id"), s(d, "name"), String::new()))
                .filter(|(id, _, _)| dcells.keys().any(|(_, c)| c == id))
                .collect(),
        );
        let fcells = families(&support, data.rows("devices"));
        let fcols = ranked(&fcells, &dids, {
            let mut v: Vec<(String, String, String)> = fcells
                .keys()
                .map(|(_, f)| (f.clone(), f.clone(), String::new()))
                .collect();
            v.dedup();
            v.sort();
            v.dedup();
            v
        });
        let pages = [
            ("index.html", "How close we are", how_close(data, g, &fam)),
            (
                "features.html",
                "Features",
                format!(
                    r#"<h1>Features</h1><p class="lead">What a phone does, against every candidate - best first, ranked by what works. Hover a cell for its source.</p>{status_legend}{}"#,
                    table("Feature", &lines, &cols, &cells)
                ),
            ),
            (
                "devices.html",
                "Devices",
                format!(
                    r#"<h1>Devices</h1><p class="lead">What works on each phone, part by part - from its lab's tests and records.</p>{status_legend}{}"#,
                    table("Component", &dlines, &dcols, &dcells)
                ),
            ),
            (
                "families.html",
                "SoC families",
                format!(
                    r#"<h1>SoC families</h1><p class="lead">Support by chip family: for each part, the best any device of the family reaches - what a new phone on the same chip can start from.</p>{status_legend}{}"#,
                    table("Component", &dlines, &fcols, &fcells)
                ),
            ),
            (
                "tests.html",
                "Tests",
                format!(
                    r#"<h1>Tests</h1><p class="lead">Every automatic test run on a device: passed, failed and skipped checks, oldest first.</p>{}<div class="scroll" style="padding:16px">{}</div>"#,
                    legend(&[("good", "passed"), ("bad", "failed"), ("none", "skipped")]),
                    runs_chart(&of(data, "test_runs", &fam))
                ),
            ),
            (
                "backlog.html",
                "Backlog",
                backlog(&of(data, "backlog", &fam)),
            ),
            (
                "landscape.html",
                "Landscape",
                landscape(&of(data, "landscape", &fam)),
            ),
        ];
        for (file, title, body) in pages {
            std::fs::write(
                dir.join(file),
                page.render(Some(g), file, &format!("{title} · {}", g.title), &body),
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
    fn grand_projects_come_from_the_registry() {
        let d = data(&[("projects", "{\"id\":\"linux-phone-oco/docs\",\"name\":\"Linux phones: as good as Android\",\"kind\":\"picture\",\"parent\":\"open-commons-observatory\",\"title\":\"Linux Phones\"}\n{\"id\":\"a/tool\",\"name\":\"t\",\"kind\":\"tool\",\"parent\":\"linux-phone-oco\"}")]);
        let g = grands(&d);
        assert_eq!(g.len(), 1, "only pictures are grand projects");
        assert_eq!(
            (
                g[0].title.as_str(),
                g[0].slug.as_str(),
                g[0].goal.as_str(),
                g[0].owner.as_str()
            ),
            (
                "Linux Phones",
                "linux-phones",
                "as good as Android",
                "linux-phone-oco"
            )
        );
        assert_eq!(
            family(&d, &g[0]).len(),
            2,
            "its own repo and those under its owner"
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
            t.contains(r#"title="Calls on X: unknown">·</td>"#),
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
            evaluate(&d, "feature:*@linux-best"),
            [
                ("calls".to_string(), "works".to_string()),
                ("sms".to_string(), "unknown".to_string())
            ]
        );
        assert_eq!(
            evaluate(&d, "tests@p")[0].1,
            "works",
            "the latest run counts"
        );
        assert_eq!(evaluate(&d, "backlog:up")[0].1, "partial");
        assert_eq!(evaluate(&d, "backlog:gone")[0].1, "unknown");
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
        };
        let e = Evidence {
            good: 1,
            part: 0,
            bad: 0,
            unknown: 0,
        };
        let svg = path_svg(&[(m("a", "working"), e), (m("b", "ongoing"), e)]);
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
}
