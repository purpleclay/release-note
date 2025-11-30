use super::{Contributor, PlatformResolver};
use anyhow::Result;
use std::collections::HashMap;

pub struct GitLabResolver {
    cache: HashMap<String, Option<Contributor>>,
    gitlab_token: Option<String>,
    project_path: String,
    graphql_url: String,
    rest_api_url: String,
}

impl GitLabResolver {
    pub fn new(project_url: &str) -> Result<Self> {
        let project_path = Self::parse_gitlab_url(project_url)?;
        let gitlab_token = std::env::var("GITLAB_TOKEN").ok();

        Ok(Self {
            cache: HashMap::new(),
            gitlab_token,
            project_path,
            graphql_url: "https://gitlab.com/api/graphql".to_string(),
            rest_api_url: "https://gitlab.com/api/v4".to_string(),
        })
    }

    #[cfg(test)]
    pub fn with_api_urls(&mut self, graphql_url: String, rest_api_url: String) {
        self.graphql_url = graphql_url;
        self.rest_api_url = rest_api_url;
    }

    fn parse_gitlab_url(url: &str) -> Result<String> {
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
            if cleaned.contains('/') {
                return Ok(cleaned.to_string());
            }
        }

        anyhow::bail!(
            "failed to extract project path from GitLab from malformed origin: {}",
            url
        )
    }

    fn extract_username_from_noreply(email: &str) -> Option<String> {
        if let Some(prefix) = email.strip_suffix("@users.noreply.gitlab.com") {
            return prefix.split('-').nth(1).map(str::to_string);
        }

        if let Some(username) = email.strip_suffix("@noreply.gitlab.com") {
            return Some(username.to_string());
        }

        None
    }

    fn normalize_graphql_query(query: &str) -> String {
        query
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn query_commit_graphql(&self, commit_hash: &str) -> Option<String> {
        let client = reqwest::blocking::Client::new();
        let query = r#"
            query GetCommitAuthor($projectPath: ID!, $ref: String!) {
                project(fullPath: $projectPath) {
                    repository {
                        commit(ref: $ref) {
                            author {
                                username
                            }
                        }
                    }
                }
            }
        "#;

        let variables = serde_json::json!({
            "projectPath": self.project_path,
            "ref": commit_hash,
        });

        let body = serde_json::json!({
            "query": Self::normalize_graphql_query(query),
            "variables": variables,
        });

        let mut request = client
            .post(&self.graphql_url)
            .header(
                "User-Agent",
                format!("release-note/{}", env!("CARGO_PKG_VERSION")),
            )
            .header("Content-Type", "application/json")
            .json(&body);

        if let Some(token) = &self.gitlab_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        match request.send() {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>() {
                        if let Some(username) = json
                            .pointer("/data/project/repository/commit/author/username")
                            .and_then(|v| v.as_str())
                        {
                            return Some(username.to_string());
                        }

                        if json
                            .pointer("/data/project/repository/commit/author")
                            .is_some_and(|v| v.is_null())
                        {
                            log::debug!(
                                "commit {} author email not linked to GitLab account",
                                &commit_hash[..7.min(commit_hash.len())]
                            );
                            return None;
                        }

                        if let Some(errors) = json.pointer("/errors") {
                            log::debug!("GraphQL errors for commit {}: {}", commit_hash, errors);
                        }
                    }
                } else {
                    log::debug!(
                        "GraphQL query failed for commit {} with status: {}",
                        &commit_hash[..7.min(commit_hash.len())],
                        resp.status()
                    );
                }
                None
            }
            Err(e) => {
                log::warn!("failed to query GitLab GraphQL API: {}", e);
                None
            }
        }
    }

    fn query_user_search(&self, username: &str) -> Option<u64> {
        let client = reqwest::blocking::Client::new();
        let search_url = format!(
            "{}/users?username={}",
            self.rest_api_url,
            urlencoding::encode(username)
        );

        let mut request = client.get(&search_url).header(
            "User-Agent",
            format!("release-note/{}", env!("CARGO_PKG_VERSION")),
        );

        if let Some(token) = &self.gitlab_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        match request.send() {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>() {
                        if let Some(user) = json.as_array().and_then(|arr| arr.first()) {
                            return user.pointer("/id").and_then(|v| v.as_u64());
                        } else {
                            log::debug!("no users found for username {}", username);
                        }
                    } else {
                        log::debug!("failed to parse user search response for {}", username);
                    }
                } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    log::debug!("user {} not found on GitLab", username);
                } else {
                    log::debug!("user search failed with status: {}", resp.status());
                }
                None
            }
            Err(e) => {
                log::warn!("failed to query GitLab user search API: {}", e);
                None
            }
        }
    }

    fn query_user_details(&self, user_id: u64) -> Option<(String, bool)> {
        let client = reqwest::blocking::Client::new();
        let details_url = format!("{}/users/{}", self.rest_api_url, user_id);

        let mut request = client.get(&details_url).header(
            "User-Agent",
            format!("release-note/{}", env!("CARGO_PKG_VERSION")),
        );

        if let Some(token) = &self.gitlab_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        match request.send() {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(user) = resp.json::<serde_json::Value>() {
                        let avatar_url = user
                            .pointer("/avatar_url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        let is_bot = user
                            .pointer("/bot")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        return Some((avatar_url, is_bot));
                    }
                } else if resp.status() == reqwest::StatusCode::FORBIDDEN {
                    log::warn!(
                        "authorization failed when querying user details for id {} (403 Forbidden)",
                        user_id
                    );
                } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    log::debug!("user details for id {} not found on GitLab", user_id);
                }
                None
            }
            Err(e) => {
                log::warn!("failed to query GitLab user details API: {}", e);
                None
            }
        }
    }

    fn query_user_api(&self, username: &str) -> Option<(String, bool)> {
        let user_id = self.query_user_search(username)?;
        self.query_user_details(user_id)
    }
}

impl PlatformResolver for GitLabResolver {
    fn resolve(&mut self, commit_hash: &str, email: &str) -> Option<Contributor> {
        log::info!("resolving contributor for email: {}", email);

        if let Some(cached) = self.cache.get(email) {
            return cached.clone();
        }

        let username = Self::resolve_ai_contributor(email)
            .or_else(|| Self::extract_username_from_noreply(email))
            .or_else(|| self.query_commit_graphql(commit_hash));

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

    const PROJECT_PATH: &str = "shakespeare/globe-theatre";
    const NESTED_PROJECT_PATH: &str = "shakespeare/tragedies/othello";
    const AVATAR_URL: &str = "https://secure.gravatar.com/avatar/test123";

    #[tokio::test]
    async fn resolves_gitlab_username_using_graphql_and_user_api() {
        use wiremock::matchers::{body_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let query = r#"
            query GetCommitAuthor($projectPath: ID!, $ref: String!) {
                project(fullPath: $projectPath) {
                    repository {
                        commit(ref: $ref) {
                            author {
                                username
                            }
                        }
                    }
                }
            }
        "#;

        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_json(serde_json::json!({
                "query": GitLabResolver::normalize_graphql_query(query),
                "variables": {
                    "projectPath": PROJECT_PATH,
                    "ref": "a1b2c3d"
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "project": {
                        "repository": {
                            "commit": {
                                "author": {
                                    "username": "hamlet"
                                }
                            }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 12345,
                    "username": "hamlet",
                    "avatar_url": AVATAR_URL
                }])),
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users/12345"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 12345,
                "username": "hamlet",
                "avatar_url": AVATAR_URL,
                "bot": false
            })))
            .mount(&mock_server)
            .await;

        let mut resolver =
            GitLabResolver::new(&format!("git@gitlab.com:{}.git", PROJECT_PATH)).unwrap();
        resolver.with_api_urls(
            format!("{}/api/graphql", mock_server.uri()),
            format!("{}/api/v4", mock_server.uri()),
        );

        let contributor = tokio::task::spawn_blocking(move || {
            resolver.resolve("a1b2c3d", "hamlet@globe-theatre.com")
        })
        .await
        .unwrap();

        assert_eq!(
            contributor,
            Some(Contributor {
                username: "hamlet".to_string(),
                avatar_url: AVATAR_URL.to_string(),
                is_bot: false,
            })
        );
    }

    #[tokio::test]
    async fn resolves_username_from_gitlab_noreply_without_graphql_call() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 22222,
                    "username": "ophelia",
                    "avatar_url": AVATAR_URL
                }])),
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users/22222"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 22222,
                "username": "ophelia",
                "avatar_url": AVATAR_URL,
                "bot": false
            })))
            .mount(&mock_server)
            .await;

        let mut resolver =
            GitLabResolver::new(&format!("https://gitlab.com/{}.git", NESTED_PROJECT_PATH))
                .unwrap();
        resolver.with_api_urls(
            format!("{}/api/graphql", mock_server.uri()),
            format!("{}/api/v4", mock_server.uri()),
        );

        let contributor = tokio::task::spawn_blocking(move || {
            resolver.resolve("e4f5g6h", "123456-ophelia@users.noreply.gitlab.com")
        })
        .await
        .unwrap();

        assert_eq!(
            contributor,
            Some(Contributor {
                username: "ophelia".to_string(),
                avatar_url: AVATAR_URL.to_string(),
                is_bot: false,
            })
        );
    }

    #[tokio::test]
    async fn resolves_ai_contributor_without_any_api_call() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 99999,
                    "username": "claude",
                    "avatar_url": AVATAR_URL
                }])),
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users/99999"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 99999,
                "username": "claude",
                "avatar_url": AVATAR_URL,
                "bot": false
            })))
            .mount(&mock_server)
            .await;

        let mut resolver =
            GitLabResolver::new(&format!("https://gitlab.com/{}.git", PROJECT_PATH)).unwrap();
        resolver.with_api_urls(
            format!("{}/api/graphql", mock_server.uri()),
            format!("{}/api/v4", mock_server.uri()),
        );

        let contributor = tokio::task::spawn_blocking(move || {
            resolver.resolve("i7j8k9l", "noreply@anthropic.com")
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

    #[tokio::test]
    async fn only_resolves_a_gitlab_username_once() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "project": {
                        "repository": {
                            "commit": {
                                "author": {
                                    "username": "othello"
                                }
                            }
                        }
                    }
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 33333,
                    "username": "othello",
                    "avatar_url": AVATAR_URL
                }])),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users/33333"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 33333,
                "username": "othello",
                "avatar_url": AVATAR_URL,
                "bot": false
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut resolver =
            GitLabResolver::new(&format!("https://gitlab.com/{}.git", PROJECT_PATH)).unwrap();
        resolver.with_api_urls(
            format!("{}/api/graphql", mock_server.uri()),
            format!("{}/api/v4", mock_server.uri()),
        );

        let (contributor1, contributor2) = tokio::task::spawn_blocking(move || {
            let contributor1 = resolver.resolve("m1n2o3p", "othello@globe-theatre.com");
            let contributor2 = resolver.resolve("q4r5s6t", "othello@globe-theatre.com");
            (contributor1, contributor2)
        })
        .await
        .unwrap();

        let expected = Some(Contributor {
            username: "othello".to_string(),
            avatar_url: AVATAR_URL.to_string(),
            is_bot: false,
        });
        assert_eq!(contributor1, expected);
        assert_eq!(contributor2, expected);
    }

    #[tokio::test]
    async fn identifies_gitlab_bot_user() {
        use wiremock::matchers::{body_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let query = r#"
            query GetCommitAuthor($projectPath: ID!, $ref: String!) {
                project(fullPath: $projectPath) {
                    repository {
                        commit(ref: $ref) {
                            author {
                                username
                            }
                        }
                    }
                }
            }
        "#;

        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_json(serde_json::json!({
                "query": GitLabResolver::normalize_graphql_query(query),
                "variables": {
                    "projectPath": PROJECT_PATH,
                    "ref": "u7v8w9x"
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "project": {
                        "repository": {
                            "commit": {
                                "author": {
                                    "username": "puck-bot"
                                }
                            }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 44444,
                    "username": "puck-bot",
                    "avatar_url": AVATAR_URL
                }])),
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v4/users/44444"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 44444,
                "username": "puck-bot",
                "avatar_url": AVATAR_URL,
                "bot": true
            })))
            .mount(&mock_server)
            .await;

        let mut resolver =
            GitLabResolver::new(&format!("git@gitlab.com:{}.git", PROJECT_PATH)).unwrap();
        resolver.with_api_urls(
            format!("{}/api/graphql", mock_server.uri()),
            format!("{}/api/v4", mock_server.uri()),
        );

        let contributor = tokio::task::spawn_blocking(move || {
            resolver.resolve("u7v8w9x", "puck-bot@globe-theatre.com")
        })
        .await
        .unwrap();

        assert_eq!(
            contributor,
            Some(Contributor {
                username: "puck-bot".to_string(),
                avatar_url: AVATAR_URL.to_string(),
                is_bot: true,
            })
        );
    }
}
