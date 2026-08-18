//! Minimal example: fetch two pages and print a summary.
//!
//! Run with:
//!   cargo run --example basic

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

    // Skills are sorted by installs by the API, so the first one is the most
    // popular.
    if let Some(top) = report.skills.first() {
        println!("Most installed: {} ({} installs)", top, top.installs);
    }

    Ok(())
}
