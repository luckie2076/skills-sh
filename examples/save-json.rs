//! Example: scrape skills and persist them as a pretty-printed JSON file.
//!
//! `Skill` implements `serde::Serialize`, so the results can be written with
//! `serde_json` (or any other serde-backed format).
//!
//! Run with:
//!   cargo run --example save-json -- <path>

use std::path::PathBuf;

use skills_sh::{scrape, ScrapeOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("skills.json"));

    let report = scrape(&ScrapeOptions {
        max_skills: Some(100),
        ..ScrapeOptions::default()
    })
    .await?;

    let json = serde_json::to_string_pretty(&report.skills)?;
    std::fs::write(&path, json)?;

    println!(
        "Saved {} skills to {}",
        report.skills.len(),
        path.display()
    );

    Ok(())
}
