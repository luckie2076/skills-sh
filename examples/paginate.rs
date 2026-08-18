//! Full pagination example: scrape until the API reports no more pages.
//!
//! Run with:
//!   cargo run --example paginate

use skills_sh::{scrape, ScrapeOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // With max_pages unset, scraping stops only when the API signals the end
    // of pagination (has_more == false).
    let report = scrape(&ScrapeOptions::default()).await?;

    println!(
        "Fetched {} skills from {} page(s)",
        report.skills.len(),
        report.pages_fetched
    );

    // Print the top 10 by install count (the API returns pages sorted by
    // installs, most popular first).
    println!("Top 10 skills:");
    for (i, skill) in report.skills.iter().take(10).enumerate() {
        println!("{:>2}. {} — {} installs", i + 1, skill, skill.installs);
    }

    Ok(())
}
