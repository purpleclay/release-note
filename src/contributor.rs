use anyhow::Result;
use std::collections::HashMap;

use crate::git::Commit;

pub trait PlatformResolver {
    fn resolve_username(&mut self, commit_hash: &str, email: &str) -> Option<String>;
}

pub struct ContributorResolver {
    platform_resolver: Box<dyn PlatformResolver>,
}

impl ContributorResolver {
    pub fn from_url(url: &str) -> Result<Option<Self>> {
        if url.contains("github.com") {
            log::info!("project is hosted on GitHub (github.com)");
            Ok(Some(Self {
                platform_resolver: Box::new(GitHubResolver::new(url)?),
            }))
        } else {
            log::warn!("unrecognized platform, contributor resolution will be skipped");
            Ok(None)
        }
    }

    pub fn resolve_contributors(&mut self, commits: &mut [Commit]) {
        for commit in commits {
            if let Some(username) = self
                .platform_resolver
                .resolve_username(&commit.hash, &commit.email)
            {
                commit.contributor = Some(username);
            }
        }
    }
}

pub struct GitHubResolver {
    cache: HashMap<String, Option<String>>,
    github_token: Option<String>,
    repo_owner: String,
    repo_name: String,
    api_url: String,
}

impl GitHubResolver {
    pub fn new(project_url: &str) -> Result<Self> {
        let (repo_owner, repo_name) = Self::parse_github_url(project_url)?;
        let github_token = std::env::var("GITHUB_TOKEN").ok();

        Ok(Self {
            cache: HashMap::new(),
            github_token,
            repo_owner,
            repo_name,
            api_url: "https://api.github.com".to_string(),
        })
    }

    #[cfg(test)]
    fn with_api_url(&mut self, api_url: String) {
        self.api_url = api_url;
    }

    fn parse_github_url(url: &str) -> Result<(String, String)> {
        let path = if url.starts_with("https://") {
            url.strip_prefix("https://")
                .and_then(|s| s.split_once('/'))
                .map(|(_, path)| path)
        } else if url.starts_with("git@") {
            url.split_once(':').map(|(_, path)| path)
        } else {
            None
        };

        if let Some(path) = path {
            let cleaned = path.trim_end_matches(".git");
            if let Some((owner, repo)) = cleaned.split_once('/')
                && !repo.contains('/')
            {
                return Ok((owner.to_string(), repo.to_string()));
            }
        }

        anyhow::bail!(
            "failed to extract project owner and name from GitHub from malformed origin: {}",
            url
        )
    }

    fn extract_username_from_noreply(email: &str) -> Option<String> {
        email
            .strip_suffix("@users.noreply.github.com")?
            .split('+')
            .nth(1)
            .map(str::to_string)
    }

    fn query_commit_api(&self, commit_hash: &str) -> Option<String> {
        let client = reqwest::blocking::Client::new();
        let url = format!(
            "{}/repos/{}/{}/commits/{}",
            self.api_url, self.repo_owner, self.repo_name, commit_hash
        );

        let mut request = client
            .get(&url)
            .header(
                "User-Agent",
                format!("release-note/{}", env!("CARGO_PKG_VERSION")),
            )
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");

        if let Some(token) = &self.github_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send();
        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>()
                        && let Some(login) = json.pointer("/author/login").and_then(|v| v.as_str())
                    {
                        return Some(login.to_string());
                    }
                } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    log::debug!(
                        "commit {} not found in project on GitHub",
                        &commit_hash[..7.min(commit_hash.len())]
                    );
                }
                None
            }
            Err(e) => {
                log::warn!("failed to query GitHub commit API: {}", e);
                None
            }
        }
    }
}

impl PlatformResolver for GitHubResolver {
    fn resolve_username(&mut self, commit_hash: &str, email: &str) -> Option<String> {
        if let Some(cached) = self.cache.get(email) {
            return cached.clone();
        }

        // First, try to extract username from GitHub noreply email
        let username = Self::extract_username_from_noreply(email)
            .or_else(|| self.query_commit_api(commit_hash));

        if username.is_some() {
            log::info!(
                "resolved username {} for email: {}",
                username.as_ref().unwrap(),
                email
            );
            self.cache.insert(email.to_string(), username.clone());
        }
        username
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO_OWNER: &str = "shakespeare";
    const REPO_NAME: &str = "globe-theatre";

    #[tokio::test]
    async fn resolves_github_username_using_commit_api() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/repos/{}/{}/commits/599e13c",
                REPO_OWNER, REPO_NAME
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "author": {
                    "login": "hamlet"
                }
            })))
            .mount(&mock_server)
            .await;

        let mut resolver =
            GitHubResolver::new(&format!("git@github.com:{}/{}.git", REPO_OWNER, REPO_NAME))
                .unwrap();
        resolver.with_api_url(mock_server.uri());

        let username = tokio::task::spawn_blocking(move || {
            resolver.resolve_username("599e13c", "hamlet@globe-theatre.com")
        })
        .await
        .unwrap();

        assert_eq!(username, Some("hamlet".to_string()));
    }

    #[tokio::test]
    async fn only_resolves_a_github_username_once() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(format!(
                r"^/repos/{}/{}/commits/[a-f0-9]+$",
                REPO_OWNER, REPO_NAME
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "author": {
                    "login": "ophelia",
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut resolver = GitHubResolver::new(&format!(
            "https://github.com/{}/{}.git",
            REPO_OWNER, REPO_NAME
        ))
        .unwrap();
        resolver.with_api_url(mock_server.uri());

        let (username1, username2) = tokio::task::spawn_blocking(move || {
            let username1 = resolver.resolve_username("3a1d4ed", "ophelia@globe-theatre.com");
            let username2 = resolver.resolve_username("cbd3d5a", "ophelia@globe-theatre.com");
            (username1, username2)
        })
        .await
        .unwrap();

        assert_eq!(username1, Some("ophelia".to_string()));
        assert_eq!(username2, Some("ophelia".to_string()));
    }

    #[tokio::test]
    async fn no_github_username_found_using_commit_api() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/repos/{}/{}/commits/da49181",
                REPO_OWNER, REPO_NAME
            )))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let mut resolver = GitHubResolver::new(&format!(
            "https://github.com/{}/{}.git",
            REPO_OWNER, REPO_NAME
        ))
        .unwrap();
        resolver.with_api_url(mock_server.uri());

        let username = tokio::task::spawn_blocking(move || {
            resolver.resolve_username("da49181", "test@example.com")
        })
        .await
        .unwrap();

        assert_eq!(username, None);
    }

    #[tokio::test]
    async fn resolves_username_from_github_noreply_email_without_api_call() {
        use wiremock::matchers::{method, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(r".*"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        let mut resolver = GitHubResolver::new(&format!(
            "https://github.com/{}/{}.git",
            REPO_OWNER, REPO_NAME
        ))
        .unwrap();
        resolver.with_api_url(mock_server.uri());

        let username = tokio::task::spawn_blocking(move || {
            resolver.resolve_username("127fca5", "12345678+prospero@users.noreply.github.com")
        })
        .await
        .unwrap();

        assert_eq!(username, Some("prospero".to_string()));
    }
}
