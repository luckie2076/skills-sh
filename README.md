# skills-sh

A minimal Rust client for scraping the [skills.sh](https://skills.sh) skill
registry.

The library exposes a single entry point, `scrape`, which paginates through
the public API (`https://skills.sh/api/skills/all-time/{page}`) until
`has_more == false`, decoding each page into strongly-typed `Skill` values.

## Quick start

Add the dependency:

```toml
[dependencies]
skills-sh = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Fetch the first two pages of skills:

```rust
use skills_sh::{scrape, ScrapeOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let report = scrape(&ScrapeOptions {
        max_pages: Some(2),
        ..ScrapeOptions::default()
    })
    .await?;

    println!(
        "Fetched {} skills from {} page(s)",
        report.skills.len(),
        report.pages_fetched
    );

    Ok(())
}
```

No client setup is required: `scrape` uses a shared default HTTP client
internally. To pull the whole registry, simply pass
`&ScrapeOptions::default()`.

## API

- **`scrape(&ScrapeOptions)`** — the only entry point. Fetches pages
  sequentially, following the API's `has_more` flag, and returns a
  `ScrapeReport { skills, pages_fetched }`.

`ScrapeOptions` fields:

| Field | Default | Meaning |
| --- | --- | --- |
| `start_page` | `0` | First page to fetch |
| `max_pages` | `None` | Hard cap on pages fetched (`None` = unbounded) |
| `max_skills` | `None` | Stop and truncate once this many skills are collected |

## Data model

Each skill carries the fields returned by the API:

| Field | Type | Notes |
| --- | --- | --- |
| `source` | `String` | Repo path (`"vercel-labs/skills"`) or provider domain (`"open.feishu.cn"`) |
| `skill_id` | `String` | Stable machine identifier, e.g. `"find-skills"` |
| `name` | `String` | Human-friendly name |
| `installs` | `u64` | Lifetime install count |
| `weekly_installs` | `Vec<u64>` | Weekly install counts |

## Examples

Ready-to-run examples live in [`examples/`](examples/):

| Example | Shows |
| --- | --- |
| [`basic`](examples/basic.rs) | Fetch two pages and print a summary |
| [`paginate`](examples/paginate.rs) | Scrape until the API reports no more pages |
| [`save-json`](examples/save-json.rs) | Persist scraped skills as pretty-printed JSON |

Run any example with:

```bash
cargo run --example basic
cargo run --example paginate
cargo run --example save-json -- /tmp/skills.json
```

## Testing

```bash
cargo test        # unit, integration and doc tests
cargo clippy --all-targets   # zero-warning lint pass
```

Tests run against a mock server, so they never touch the live API.

## License

MIT
