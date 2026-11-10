# oco

The engine of the [Open Commons Observatory](https://github.com/open-commons-observatory/open-commons-observatory.github.io):
the mark (`oco.toml`), discovery, the harvest, and the site renderer - in Rust. How the pieces fit:
the observatory's [ARCHITECTURE.md](https://github.com/open-commons-observatory/open-commons-observatory.github.io/blob/main/ARCHITECTURE.md).

    oco discover [--kind KIND] [--json]    every marked project, each confirmed by its oco.toml

Discovery is hexagonal: an `Outlet` finds candidates on one platform, a `Source` reads their files;
GitHub is the first (the `oco-project` topic, `oco-kind-<kind>` to narrow), others plug in later.
Set `GITHUB_TOKEN` for higher rate limits.
