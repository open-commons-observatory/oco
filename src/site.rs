//! The observatory's face: a designed static website from the harvested tad data - every word and
//! number from the data, no JavaScript. Pages: the overview, devices, features, tests, the backlog,
//! the landscape; one stylesheet; graphics as inline SVG.

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

type Row = Map<String, Value>;

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

/// What a status means, one of five, each with one color across the site.
pub fn tone(status: &str) -> &'static str {
    match status {
        "works" | "done" | "pass" | "yes" => "good",
        "partial" | "in-progress" => "part",
        "broken" | "missing" | "fail" | "no" => "bad",
        "not-applicable" | "n/a" => "na",
        _ => "none",
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

    /// id -> (name, category) from a reference table; the id stands in when it has no name.
    fn refs(&self, t: &str) -> Vec<(String, String, String)> {
        self.rows(t)
            .iter()
            .map(|r| {
                let id = s(r, "id");
                let name = Some(s(r, "name"))
                    .filter(|x| !x.is_empty())
                    .unwrap_or_else(|| id.clone());
                (id, name, s(r, "category"))
            })
            .collect()
    }
}

const STYLE: &str = r#":root{--bg:#fbfaf8;--fg:#1d1d1f;--mute:#6e6e73;--line:#e6e3de;--card:#fff;--accent:#3b5bdb;
--good:#2f9e6b;--part:#d9a43b;--bad:#d1495b;--none:#c9ccd1;--na:#ecedef}
@media(prefers-color-scheme:dark){:root{--bg:#141416;--fg:#ececf0;--mute:#9a9aa3;--line:#2a2a2f;--card:#1c1c20;--accent:#8ea2ff;--none:#4a4c52;--na:#2a2b30}}
*{box-sizing:border-box}html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.6 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
a{color:var(--accent);text-decoration:none}a:hover{text-decoration:underline}
.wrap{max-width:1120px;margin:0 auto;padding:0 24px}
header{border-bottom:1px solid var(--line);background:var(--bg);position:sticky;top:0;z-index:2}
header .wrap{display:flex;align-items:center;gap:28px;height:60px;overflow-x:auto}
.mark{font-weight:650;letter-spacing:-.01em;white-space:nowrap;color:var(--fg)}
nav{display:flex;gap:20px;font-size:15px}nav a{color:var(--mute);white-space:nowrap}nav a.on{color:var(--fg);font-weight:600}
main{padding:48px 0 72px}h1{font-size:40px;line-height:1.15;letter-spacing:-.02em;margin:0 0 12px;font-weight:700}
h2{font-size:22px;letter-spacing:-.01em;margin:48px 0 16px}h3{font-size:15px;margin:28px 0 10px;color:var(--mute);text-transform:uppercase;letter-spacing:.06em}
.lead{font-size:19px;color:var(--mute);max-width:720px;margin:0}
.stats{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:16px;margin-top:36px}
.stat{background:var(--card);border:1px solid var(--line);border-radius:14px;padding:18px 20px}
.stat b{display:block;font-size:30px;letter-spacing:-.02em}.stat span{color:var(--mute);font-size:14px}
.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:16px}
.card{background:var(--card);border:1px solid var(--line);border-radius:14px;padding:18px 20px}
.card h4{margin:0 0 6px;font-size:17px}.card p{margin:6px 0 0;color:var(--mute);font-size:14.5px}
.chip{display:inline-block;font-size:12px;padding:2px 9px;border-radius:99px;border:1px solid var(--line);color:var(--mute);margin-right:6px}
.log{border-left:2px solid var(--line);padding:2px 0 2px 18px;margin:18px 0}.log time{color:var(--mute);font-size:13px}
.log h4{margin:2px 0 4px;font-size:16px}.log p{margin:0;color:var(--mute);font-size:14.5px}
.matrix{border-collapse:separate;border-spacing:4px;font-size:14px}.matrix th{font-weight:500;color:var(--mute);text-align:left;padding:0 10px 0 0;white-space:nowrap}
.matrix thead th{writing-mode:vertical-rl;transform:rotate(180deg);height:120px;padding:0 0 6px;text-align:left}
.cell{width:26px;height:26px;border-radius:7px;background:var(--none)}
.good{background:var(--good)}.part{background:var(--part)}.bad{background:var(--bad)}.none{background:var(--none)}.na{background:var(--na)}
.legend{display:flex;gap:18px;flex-wrap:wrap;color:var(--mute);font-size:14px;margin:8px 0 24px}.legend i{display:inline-block;width:12px;height:12px;border-radius:4px;margin-right:6px;vertical-align:-1px}
.scroll{overflow-x:auto}
.board{display:grid;grid-template-columns:repeat(3,1fr);gap:20px}@media(max-width:820px){.board{grid-template-columns:1fr}}
.col h3{margin-top:0}.col .card{margin-bottom:12px}
footer{border-top:1px solid var(--line);color:var(--mute);font-size:14px;padding:28px 0}
"#;

const PAGES: [(&str, &str); 6] = [
    ("index.html", "Overview"),
    ("devices.html", "Devices"),
    ("features.html", "Features"),
    ("tests.html", "Tests"),
    ("backlog.html", "Backlog"),
    ("landscape.html", "Landscape"),
];

fn page(file: &str, title: &str, body: &str, generated: &str) -> String {
    let mut nav = String::new();
    for (f, t) in PAGES {
        let _ = write!(
            nav,
            r#"<a href="{f}"{}>{t}</a>"#,
            if f == file { r#" class="on""# } else { "" }
        );
    }
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{t} · Open Commons Observatory</title><link rel="stylesheet" href="style.css"></head><body>
<header><div class="wrap"><a class="mark" href="index.html">Open Commons Observatory</a><nav>{nav}</nav></div></header>
<main><div class="wrap">{body}</div></main>
<footer><div class="wrap">Every word and number here comes from the projects' own data, gathered by the observatory · {g}</div></footer>
</body></html>
"#,
        t = esc(title),
        g = esc(generated)
    )
}

fn legend(items: &[(&str, &str)]) -> String {
    let mut l = String::from(r#"<div class="legend">"#);
    for (t, label) in items {
        let _ = write!(l, r#"<span><i class="{t}"></i>{label}</span>"#);
    }
    l + "</div>"
}

/// A matrix of statuses: rows from `row_ref` grouped by category, columns from `col_ref`.
pub fn matrix(
    data: &Data,
    table: &str,
    row_key: &str,
    row_ref: &str,
    col_key: &str,
    col_ref: &str,
) -> String {
    let cells: BTreeMap<(String, String), (String, String)> = data
        .rows(table)
        .iter()
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
        .collect();
    let used_cols: Vec<_> = data
        .refs(col_ref)
        .into_iter()
        .filter(|(id, _, _)| cells.keys().any(|(_, c)| c == id))
        .collect();
    let mut by_cat: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut cat_order: Vec<String> = Vec::new();
    for (id, name, cat) in data.refs(row_ref) {
        if !cells.keys().any(|(r, _)| *r == id) {
            continue;
        }
        if !cat_order.contains(&cat) {
            cat_order.push(cat.clone());
        }
        by_cat.entry(cat).or_default().push((id, name));
    }
    let mut out = String::new();
    for cat in cat_order {
        let _ = write!(
            out,
            "<h3>{}</h3><div class=\"scroll\"><table class=\"matrix\"><thead><tr><th></th>",
            esc(if cat.is_empty() { "other" } else { &cat })
        );
        for (_, cname, _) in &used_cols {
            let _ = write!(out, "<th>{}</th>", esc(cname));
        }
        out.push_str("</tr></thead><tbody>");
        for (rid, rname) in &by_cat[&cat] {
            let _ = write!(out, "<tr><th>{}</th>", esc(rname));
            for (cid, cname, _) in &used_cols {
                let (status, note) = cells
                    .get(&(rid.clone(), cid.clone()))
                    .cloned()
                    .unwrap_or_else(|| ("unknown".into(), String::new()));
                let tip = if note.is_empty() {
                    format!("{rname} on {cname}: {status}")
                } else {
                    format!("{rname} on {cname}: {status} - {note}")
                };
                let _ = write!(
                    out,
                    r#"<td><div class="cell {}" title="{}"></div></td>"#,
                    tone(&status),
                    esc(&cut(&tip, 300))
                );
            }
            out.push_str("</tr>");
        }
        out.push_str("</tbody></table></div>");
    }
    out
}

fn share(data: &Data, table: &str) -> (usize, usize) {
    let rows = data.rows(table);
    (
        rows.iter()
            .filter(|r| tone(&s(r, "status")) == "good")
            .count(),
        rows.len(),
    )
}

fn overview(data: &Data) -> String {
    let projects = data.rows("projects");
    let (fw, ft) = share(data, "feature_support");
    let (dw, dt) = share(data, "support_status");
    let open = data
        .rows("backlog")
        .iter()
        .filter(|r| s(r, "status") != "done")
        .count();
    let pct = |a: usize, b: usize| (a * 100).checked_div(b).unwrap_or(0);
    let mut b = format!(
        r#"<h1>One living picture of the open commons.</h1><p class="lead">Every project here keeps its own state as data; the observatory finds the marked ones and gathers what they share - nothing is wired in by hand.</p>
<div class="stats"><div class="stat"><b>{}</b><span>projects in the picture</span></div><div class="stat"><b>{}%</b><span>of features working, across platforms</span></div><div class="stat"><b>{}%</b><span>of device components working</span></div><div class="stat"><b>{}</b><span>open items in the backlog</span></div></div>"#,
        projects.len(),
        pct(fw, ft),
        pct(dw, dt),
        open
    );
    let ids: Vec<String> = projects.iter().map(|p| s(p, "id")).collect();
    let mut parents: Vec<String> = Vec::new();
    for p in projects {
        let par = s(p, "parent");
        if !parents.contains(&par) {
            parents.push(par);
        }
    }
    b.push_str("<h2>Projects</h2>");
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
                r#"<div class="card"><h4><a href="{}">{}</a></h4><span class="chip">{}</span><span class="chip">{} shared tables</span><p>{}</p></div>"#,
                esc(&s(p, "link")),
                esc(&s(p, "name")),
                esc(&s(p, "kind")),
                tables,
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
            r#"<div class="log"><time>{}</time><h4>{}</h4><p>{}</p></div>"#,
            esc(&s(r, "entry_date")),
            esc(&s(r, "title")),
            esc(&cut(&s(r, "done"), 280))
        );
    }
    b
}

/// The test runs as stacked bars - passed, failed, skipped - oldest first.
pub fn runs_chart(data: &Data) -> String {
    let mut runs: Vec<&Row> = data.rows("test_runs").iter().collect();
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

fn backlog(data: &Data) -> String {
    let mut b = String::from(
        r#"<h1>Backlog</h1><p class="lead">The work ahead and done, across every project.</p><div class="board" style="margin-top:32px">"#,
    );
    for (st, title) in [
        ("open", "Open"),
        ("in-progress", "In progress"),
        ("done", "Done"),
    ] {
        let items: Vec<&Row> = data
            .rows("backlog")
            .iter()
            .filter(|r| s(r, "status") == st)
            .collect();
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

fn landscape(data: &Data) -> String {
    let mut b = String::from(
        r#"<h1>Landscape</h1><p class="lead">The projects around ours - what each does, and how we build on it rather than beside it.</p><div class="cards" style="margin-top:32px">"#,
    );
    for r in data.rows("landscape") {
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

/// Renders every page into `out`.
pub fn render(data: &Data, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let generated = format!("updated {}", utc(now));
    let feature_legend = legend(&[
        ("good", "works"),
        ("part", "partly"),
        ("bad", "missing"),
        ("none", "not known yet"),
        ("na", "not applicable"),
    ]);
    let device_legend = legend(&[
        ("good", "works"),
        ("part", "partly"),
        ("bad", "broken"),
        ("none", "not known yet"),
    ]);
    let pages = [
        ("index.html", "Overview", overview(data)),
        (
            "devices.html",
            "Devices",
            format!(
                r#"<h1>Devices</h1><p class="lead">What works on each phone, part by part - from its lab's tests and records.</p>{device_legend}{}"#,
                matrix(
                    data,
                    "support_status",
                    "component_id",
                    "components",
                    "device_id",
                    "devices"
                )
            ),
        ),
        (
            "features.html",
            "Features",
            format!(
                r#"<h1>Features</h1><p class="lead">What a phone does, against the platforms that could do it.</p>{feature_legend}{}"#,
                matrix(
                    data,
                    "feature_support",
                    "feature_id",
                    "features",
                    "platform_id",
                    "platforms"
                )
            ),
        ),
        (
            "tests.html",
            "Tests",
            format!(
                r#"<h1>Tests</h1><p class="lead">Every automatic test run on a device: passed, failed and skipped checks, oldest first.</p>{}{}"#,
                legend(&[("good", "passed"), ("bad", "failed"), ("none", "skipped")]),
                runs_chart(data)
            ),
        ),
        ("backlog.html", "Backlog", backlog(data)),
        ("landscape.html", "Landscape", landscape(data)),
    ];
    for (file, title, body) in pages {
        std::fs::write(out.join(file), page(file, title, &body, &generated))?;
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
    fn text_is_escaped() {
        assert_eq!(
            esc("<a href=\"x\">&</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;"
        );
    }

    #[test]
    fn every_status_has_one_meaning() {
        assert_eq!(tone("works"), "good");
        assert_eq!(tone("in-progress"), "part");
        assert_eq!(tone("missing"), "bad");
        assert_eq!(tone("not-applicable"), "na");
        assert_eq!(tone("anything else"), "none");
    }

    #[test]
    fn a_matrix_shows_every_row_and_an_empty_cell_as_unknown() {
        let d = data(&[
            ("features", "{\"id\":\"calls\",\"name\":\"Calls\",\"category\":\"phone\"}\n{\"id\":\"sms\",\"name\":\"SMS\",\"category\":\"phone\"}"),
            ("platforms", "{\"id\":\"pmos\",\"name\":\"postmarketOS\"}\n{\"id\":\"ios\",\"name\":\"iOS\"}"),
            ("feature_support", "{\"feature_id\":\"calls\",\"platform_id\":\"pmos\",\"status\":\"works\"}\n{\"feature_id\":\"sms\",\"platform_id\":\"ios\",\"status\":\"missing\"}"),
        ]);
        let m = matrix(
            &d,
            "feature_support",
            "feature_id",
            "features",
            "platform_id",
            "platforms",
        );
        assert!(m.contains("Calls") && m.contains("SMS") && m.contains("postmarketOS"));
        assert!(m.contains(r#"class="cell good" title="Calls on postmarketOS: works""#));
        assert!(
            m.contains(r#"title="Calls on iOS: unknown""#),
            "a cell with no row is unknown, not blank"
        );
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
        let c = runs_chart(&d);
        assert_eq!(c.matches("<rect").count(), 3);
        assert!(c.contains("<title>9 passed</title>"));
    }
}
