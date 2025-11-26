use anyhow::Result;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashMap;

use crate::git::Commit;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct Contributor {
    pub username: String,
    pub avatar_url: String,
    pub is_bot: bool,
}

pub trait PlatformResolver {
    fn resolve(&mut self, commit_hash: &str, email: &str) -> Option<Contributor>;
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
        use crate::git::GitTrailer;

        for commit in commits {
            if let Some(contributor) = self.platform_resolver.resolve(&commit.hash, &commit.email) {
                commit.contributors.push(contributor);
            }

            for trailer in &commit.trailers {
                if let GitTrailer::CoAuthoredBy { name: _, email } = trailer
                    && let Some(email_addr) = email
                    && let Some(contributor) =
                        self.platform_resolver.resolve(&commit.hash, email_addr)
                    && !commit
                        .contributors
                        .iter()
                        .any(|c| c.username == contributor.username)
                {
                    commit.contributors.push(contributor);
                }
            }
        }
    }
}

pub struct GitHubResolver {
    cache: HashMap<String, Option<Contributor>>,
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

    fn resolve_ai_contributor(email: &str) -> Option<String> {
        /// Known AI assistant emails and their GitHub handles.
        ///
        /// Currently supported:
        /// - Claude: Uses `noreply@anthropic.com` as documented in Claude Code
        ///   (See: https://github.com/anthropics/claude-code/issues/1653)
        ///   Note: As of June 2025, there were issues with this email being claimed
        ///   by an unrelated GitHub user. Anthropic is aware of this issue.
        ///   (See: https://github.com/anthropics/claude-code/issues/1655)
        static AI_CONTRIBUTORS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
            HashMap::from([
                // Claude Code uses this email for co-authorship attribution
                // Format: Co-authored-by: Claude <noreply@anthropic.com>
                ("noreply@anthropic.com", "claude"),
            ])
        });

        AI_CONTRIBUTORS.get(email).map(|username| {
            log::info!("Resolved AI contributor: {} -> @{}", email, username);
            username.to_string()
        })
    }

    fn extract_username_from_noreply(email: &str) -> Option<String> {
        email
            .strip_suffix("@users.noreply.github.com")?
            .split('+')
            .nth(1)
            .map(str::to_string)
    }

    fn query_user_api(&self, username: &str) -> Option<(String, bool)> {
        let client = reqwest::blocking::Client::new();
        let url = format!("{}/users/{}", self.api_url, urlencoding::encode(username));

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

        match request.send() {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>()
                        && let Some(avatar_url) =
                            json.pointer("/avatar_url").and_then(|v| v.as_str())
                    {
                        let is_bot = json
                            .pointer("/type")
                            .and_then(|v| v.as_str())
                            .map(|t| t.eq_ignore_ascii_case("Bot"))
                            .unwrap_or(false);

                        return Some((avatar_url.to_string(), is_bot));
                    }
                } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    log::debug!("user {} not found on GitHub", username);
                }
                None
            }
            Err(e) => {
                log::warn!("failed to query GitHub user API: {}", e);
                None
            }
        }
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
    fn resolve(&mut self, commit_hash: &str, email: &str) -> Option<Contributor> {
        if let Some(cached) = self.cache.get(email) {
            return cached.clone();
        }

        let username = Self::resolve_ai_contributor(email)
            .or_else(|| Self::extract_username_from_noreply(email))
            .or_else(|| self.query_commit_api(commit_hash));

        let contributor = username.map(|username| {
            let (avatar_url, is_bot) = self
                .query_user_api(&username)
                .unwrap_or_else(|| (String::new(), false));

            log::info!(
                "resolved contributor {} for email: {} (bot: {})",
                username,
                email,
                is_bot
            );

            Contributor {
                username,
                avatar_url,
                is_bot,
            }
        });

        self.cache.insert(email.to_string(), contributor.clone());
        contributor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO_OWNER: &str = "shakespeare";
    const REPO_NAME: &str = "globe-theatre";
    const AVATAR_URL: &str = "https://avatars.githubusercontent.com/u/2651292?v=4";

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
                    "login": "hamlet[bot]"
                }
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/users/{}",
                urlencoding::encode("hamlet[bot]")
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "avatar_url": AVATAR_URL,
                "type": "Bot",
            })))
            .mount(&mock_server)
            .await;

        let mut resolver =
            GitHubResolver::new(&format!("git@github.com:{}/{}.git", REPO_OWNER, REPO_NAME))
                .unwrap();
        resolver.with_api_url(mock_server.uri());

        let contributor = tokio::task::spawn_blocking(move || {
            resolver.resolve("599e13c", "hamlet[bot]@globe-theatre.com")
        })
        .await
        .unwrap();

        assert_eq!(
            contributor,
            Some(Contributor {
                username: "hamlet[bot]".to_string(),
                avatar_url: AVATAR_URL.to_string(),
                is_bot: true,
            })
        );
    }

    #[tokio::test]
    async fn only_resolves_a_github_username_once() {
        use wiremock::matchers::{method, path, path_regex};
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

        Mock::given(method("GET"))
            .and(path("/users/ophelia"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "avatar_url": AVATAR_URL
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

        let (contributor1, contributor2) = tokio::task::spawn_blocking(move || {
            let contributor1 = resolver.resolve("3a1d4ed", "ophelia@globe-theatre.com");
            let contributor2 = resolver.resolve("cbd3d5a", "ophelia@globe-theatre.com");
            (contributor1, contributor2)
        })
        .await
        .unwrap();

        let expected = Some(Contributor {
            username: "ophelia".to_string(),
            avatar_url: AVATAR_URL.to_string(),
            is_bot: false,
        });
        assert_eq!(contributor1, expected);
        assert_eq!(contributor2, expected);
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

        let username =
            tokio::task::spawn_blocking(move || resolver.resolve("da49181", "test@example.com"))
                .await
                .unwrap();

        assert_eq!(username, None);
    }

    #[tokio::test]
    async fn resolves_username_from_github_noreply_email_without_commit_api_call() {
        use wiremock::matchers::{method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(format!(
                r"^/repos/{}/{}/commits/",
                REPO_OWNER, REPO_NAME
            )))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/users/prospero"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "avatar_url": AVATAR_URL
            })))
            .mount(&mock_server)
            .await;

        let mut resolver = GitHubResolver::new(&format!(
            "https://github.com/{}/{}.git",
            REPO_OWNER, REPO_NAME
        ))
        .unwrap();
        resolver.with_api_url(mock_server.uri());

        let contributor = tokio::task::spawn_blocking(move || {
            resolver.resolve("127fca5", "12345678+prospero@users.noreply.github.com")
        })
        .await
        .unwrap();

        assert_eq!(
            contributor,
            Some(Contributor {
                username: "prospero".to_string(),
                avatar_url: AVATAR_URL.to_string(),
                is_bot: false,
            })
        );
    }

    #[tokio::test]
    async fn resolves_ai_contributor_without_commit_api_call() {
        use wiremock::matchers::{method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(format!(
                r"^/repos/{}/{}/commits/",
                REPO_OWNER, REPO_NAME
            )))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/users/claude"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "avatar_url": AVATAR_URL
            })))
            .mount(&mock_server)
            .await;

        let mut resolver = GitHubResolver::new(&format!(
            "https://github.com/{}/{}.git",
            REPO_OWNER, REPO_NAME
        ))
        .unwrap();
        resolver.with_api_url(mock_server.uri());

        let contributor = tokio::task::spawn_blocking(move || {
            resolver.resolve("f6ab8dd", "noreply@anthropic.com")
        })
        .await
        .unwrap();

        assert_eq!(
            contributor,
            Some(Contributor {
                username: "claude".to_string(),
                avatar_url: AVATAR_URL.to_string(),
                is_bot: false,
            })
        );
    }
}
