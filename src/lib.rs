//! skills-sh: a minimal async client for scraping the
//! [skills.sh](https://skills.sh) skill registry.
//!
//! The library exposes a single entry point, [`scrape`], which paginates
//! through the public API (`https://skills.sh/api/skills/all-time/{page}`),
//! decoding each page into strongly-typed [`Skill`] values and following the
//! API's `has_more` flag until the last page.
//!
//! # Example
//!
//! ```no_run
//! use skills_sh::{scrape, ScrapeOptions};
//!
//! # async fn run() -> Result<(), skills_sh::Error> {
//! let report = scrape(&ScrapeOptions {
//!     max_pages: Some(2),
//!     ..ScrapeOptions::default()
//! })
//! .await?;
//! println!("fetched {} skills", report.skills.len());
//! # Ok(())
//! # }
//! ```

use std::fmt;
use std::sync::OnceLock;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

/// Default base URL of the skills.sh paginated API.
pub const DEFAULT_API_BASE: &str = "https://skills.sh/api/skills/all-time";

/// Default HTTP `User-Agent` sent with every request.
const USER_AGENT: &str = concat!("skills-sh/", env!("CARGO_PKG_VERSION"));

/// The lazily-created shared HTTP client reused by every [`scrape`] call.
///
/// A single connection pool is shared across the whole process, so repeated
/// scrapes reuse keep-alive connections instead of rebuilding them.
fn shared_client() -> &'static reqwest::Client {
    static SHARED: OnceLock<reqwest::Client> = OnceLock::new();
    SHARED.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to build default HTTP client")
    })
}

/// A single skill entry as returned by the skills.sh API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    /// Source repository path or provider domain,
    /// e.g. `"vercel-labs/skills"` or `"open.feishu.cn"`.
    pub source: String,
    /// Stable machine identifier of the skill, e.g. `"find-skills"`.
    pub skill_id: String,
    /// Human-friendly name of the skill.
    pub name: String,
    /// Lifetime install count.
    pub installs: u64,
    /// Weekly install counts.
    pub weekly_installs: Vec<u64>,
}

/// One page of the paginated API response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Page {
    /// Skills contained in this page.
    pub skills: Vec<Skill>,
    /// Whether more pages are available after this one.
    #[serde(default, rename = "hasMore")]
    pub has_more: bool,
    /// Total number of skills across all pages, when the API provides it.
    #[serde(default)]
    pub total: Option<u64>,
}

/// Errors returned by the skills.sh client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Transport-level failure (DNS, connect, timeout, TLS, ...).
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    /// The API answered with a non-success HTTP status.
    #[error("page {page} returned HTTP {status}")]
    BadStatus { page: u64, status: StatusCode },
    /// The response body could not be decoded as a skills page.
    #[error("invalid response for page {page}: {source}")]
    InvalidResponse {
        page: u64,
        #[source]
        source: serde_json::Error,
    },
}

/// Options controlling [`scrape`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScrapeOptions {
    /// First page to fetch. Defaults to `0`.
    pub start_page: u64,
    /// Hard cap on the number of pages fetched, counting from `start_page`.
    /// `None` means "keep going until the API reports `has_more == false`".
    pub max_pages: Option<u64>,
    /// Stop once this many skills have been collected. The returned list is
    /// truncated to exactly this length. `None` means "no limit".
    pub max_skills: Option<usize>,
}

/// The outcome of [`scrape`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapeReport {
    /// All skills collected across the fetched pages.
    pub skills: Vec<Skill>,
    /// Number of pages actually fetched.
    pub pages_fetched: u64,
}

/// Scrape skills from the skills.sh registry.
///
/// Fetches pages sequentially, following the API's `has_more` flag until the
/// last page or until an option limit is reached. Uses a shared default HTTP
/// client, so no client setup is required.
pub async fn scrape(opts: &ScrapeOptions) -> Result<ScrapeReport, Error> {
    scrape_with(shared_client(), DEFAULT_API_BASE, opts).await
}

/// Fetch a single page from `base_url`.
async fn fetch_page(http: &reqwest::Client, base_url: &str, page: u64) -> Result<Page, Error> {
    let url = format!("{base_url}/{page}");
    let res = http.get(&url).send().await?;
    if !res.status().is_success() {
        return Err(Error::BadStatus {
            page,
            status: res.status(),
        });
    }
    let body = res.text().await?;
    serde_json::from_str(&body).map_err(|source| Error::InvalidResponse { page, source })
}

/// Scrape against an explicit client and base URL, so tests and mirrors can
/// point the pipeline anywhere without exposing a client type.
async fn scrape_with(
    http: &reqwest::Client,
    base_url: &str,
    opts: &ScrapeOptions,
) -> Result<ScrapeReport, Error> {
    let mut skills = Vec::new();
    let mut pages_fetched = 0u64;
    let mut page = opts.start_page;

    loop {
        if let Some(max_pages) = opts.max_pages
            && pages_fetched >= max_pages
        {
            break;
        }
        let data = fetch_page(http, base_url, page).await?;
        pages_fetched += 1;
        let has_more = data.has_more;
        skills.extend(data.skills);

        // Truncate before the `has_more` check so `max_skills` holds even on
        // the final page (when the API signals the end of pagination).
        if let Some(max_skills) = opts.max_skills
            && skills.len() >= max_skills
        {
            skills.truncate(max_skills);
            break;
        }
        if !has_more {
            break;
        }
        page += 1;
    }

    Ok(ScrapeReport {
        skills,
        pages_fetched,
    })
}

impl fmt::Display for Skill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.source, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Two skills on page 0, with `hasMore = true`. Note the API may return
    /// extra fields (e.g. `isOfficial`) which serde silently ignores.
    const PAGE_0: &str = r#"{
        "skills": [
            {
                "source": "vercel-labs/skills",
                "skillId": "find-skills",
                "name": "find-skills",
                "installs": 2991984,
                "weeklyInstalls": [113781, 109199, 109085],
                "isOfficial": true
            },
            {
                "source": "mattpocock/skills",
                "skillId": "grill-me",
                "name": "grill-me",
                "installs": 885684,
                "weeklyInstalls": [47001]
            }
        ],
        "hasMore": true,
        "total": 3
    }"#;

    /// One skill on page 1, signalling the end of pagination.
    const PAGE_1: &str = r#"{
        "skills": [
            {
                "source": "anthropics/skills",
                "skillId": "frontend-design",
                "name": "frontend-design",
                "installs": 787725,
                "weeklyInstalls": [27857]
            }
        ],
        "hasMore": false,
        "total": 3
    }"#;

    fn page_mock(page: u64, body: &'static str) -> Mock {
        Mock::given(method("GET"))
            .and(path(format!("/api/skills/all-time/{page}")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
    }

    async fn mock_http() -> reqwest::Client {
        reqwest::Client::new()
    }

    fn mock_base(server: &MockServer) -> String {
        format!("{}/api/skills/all-time", server.uri())
    }

    #[tokio::test]
    async fn decodes_page_from_api_shape() {
        let server = MockServer::start().await;
        page_mock(0, PAGE_0).mount(&server).await;

        let page = fetch_page(&mock_http().await, &mock_base(&server), 0)
            .await
            .unwrap();

        assert!(page.has_more);
        assert_eq!(page.total, Some(3));
        assert_eq!(page.skills.len(), 2);

        let skill = &page.skills[0];
        assert_eq!(skill.source, "vercel-labs/skills");
        assert_eq!(skill.skill_id, "find-skills");
        assert_eq!(skill.installs, 2_991_984);
        assert_eq!(skill.weekly_installs, vec![113_781, 109_199, 109_085]);
    }

    #[tokio::test]
    async fn fetch_page_rejects_bad_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/skills/all-time/0"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let err = fetch_page(&mock_http().await, &mock_base(&server), 0)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::BadStatus { page: 0, status: _ }));
    }

    #[tokio::test]
    async fn fetch_page_rejects_malformed_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/skills/all-time/0"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("not json", "application/json"))
            .mount(&server)
            .await;

        let err = fetch_page(&mock_http().await, &mock_base(&server), 0)
            .await
            .unwrap_err();

        assert!(matches!(err, Error::InvalidResponse { page: 0, .. }));
    }

    #[tokio::test]
    async fn scrape_follows_has_more_until_last_page() {
        let server = MockServer::start().await;
        page_mock(0, PAGE_0).mount(&server).await;
        page_mock(1, PAGE_1).mount(&server).await;

        let report = scrape_with(
            &mock_http().await,
            &mock_base(&server),
            &ScrapeOptions::default(),
        )
        .await
        .unwrap();

        assert_eq!(report.pages_fetched, 2);
        assert_eq!(report.skills.len(), 3);
        assert_eq!(
            report
                .skills
                .iter()
                .map(|s| s.skill_id.as_str())
                .collect::<Vec<_>>(),
            vec!["find-skills", "grill-me", "frontend-design"]
        );
    }

    #[tokio::test]
    async fn scrape_respects_max_pages() {
        let server = MockServer::start().await;
        page_mock(0, PAGE_0).mount(&server).await;
        // Page 1 must never be requested.
        Mock::given(method("GET"))
            .and(path("/api/skills/all-time/1"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let report = scrape_with(
            &mock_http().await,
            &mock_base(&server),
            &ScrapeOptions {
                max_pages: Some(1),
                ..ScrapeOptions::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(report.pages_fetched, 1);
        assert_eq!(report.skills.len(), 2);
    }

    #[tokio::test]
    async fn scrape_respects_max_skills_with_more_pages() {
        let server = MockServer::start().await;
        page_mock(0, PAGE_0).mount(&server).await;
        page_mock(1, PAGE_1).mount(&server).await;

        let report = scrape_with(
            &mock_http().await,
            &mock_base(&server),
            &ScrapeOptions {
                max_skills: Some(2),
                ..ScrapeOptions::default()
            },
        )
        .await
        .unwrap();

        // Page 0 already holds >= 2 skills with has_more = true.
        assert_eq!(report.pages_fetched, 1);
        assert_eq!(report.skills.len(), 2);
    }

    #[tokio::test]
    async fn scrape_truncates_on_final_page() {
        // Regression test: `max_skills` must hold even when the page that
        // overflows the limit is also the last one (has_more == false).
        let server = MockServer::start().await;
        page_mock(0, PAGE_0).mount(&server).await;
        page_mock(1, PAGE_1).mount(&server).await;

        let report = scrape_with(
            &mock_http().await,
            &mock_base(&server),
            &ScrapeOptions {
                max_skills: Some(2),
                ..ScrapeOptions::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(report.skills.len(), 2);
    }

    #[test]
    fn scrape_options_defaults() {
        assert_eq!(
            ScrapeOptions::default(),
            ScrapeOptions {
                start_page: 0,
                max_pages: None,
                max_skills: None,
            }
        );
    }
}
