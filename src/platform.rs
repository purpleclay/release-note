use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    GitHub {
        url: String,
        api_url: String,
        owner: String,
        repo: String,
        token: Option<String>,
    },
    GitLab {
        url: String,
        api_url: String,
        graphql_url: String,
        project_path: String,
        token: Option<String>,
    },
    Unknown,
}

impl Platform {
    pub fn detect(origin_url: Option<&str>) -> Self {
        let platform = if let Some(platform) = Self::from_ci_env() {
            platform
        } else {
            match origin_url {
                Some(url) => Self::from_origin_url(url),
                None => {
                    log::warn!("no origin URL and not running in CI");
                    return Platform::Unknown;
                }
            }
        };

        match platform {
            Platform::GitHub {
                url,
                api_url,
                owner,
                repo,
                ..
            } => {
                let token = std::env::var("GITHUB_TOKEN").ok();
                if token.is_none() {
                    log::warn!("no GITHUB_TOKEN found; API requests may be rate limited");
                }
                Platform::GitHub {
                    url,
                    api_url,
                    owner,
                    repo,
                    token,
                }
            }
            Platform::GitLab {
                url,
                api_url,
                graphql_url,
                project_path,
                ..
            } => {
                let token = std::env::var("GITLAB_TOKEN").ok();
                if token.is_none() {
                    log::warn!(
                        "no GITLAB_TOKEN found; contributor resolution requires a token with 'read_user' scope"
                    );
                }
                Platform::GitLab {
                    url,
                    api_url,
                    graphql_url,
                    project_path,
                    token,
                }
            }
            Platform::Unknown => Platform::Unknown,
        }
    }

    fn extract_host_with_protocol(url: &str) -> Option<(String, String)> {
        let protocol = if url.starts_with("https://") {
            "https"
        } else if url.starts_with("http://") {
            "http"
        } else {
            return None;
        };

        let without_protocol = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))?;

        let host = without_protocol.split('/').next()?;
        Some((protocol.to_string(), host.to_string()))
    }

    fn from_ci_env() -> Option<Self> {
        if std::env::var("GITLAB_CI").is_ok()
            && let Ok(url) = std::env::var("CI_PROJECT_URL")
        {
            let api_url = std::env::var("CI_API_V4_URL").unwrap_or_else(|_| {
                if let Some((protocol, host)) = Self::extract_host_with_protocol(&url) {
                    return Self::infer_gitlab_api_url(&protocol, &host);
                }
                format!("{}/api/v4", url.trim_end_matches('/'))
            });

            let graphql_url = std::env::var("CI_API_GRAPHQL_URL").unwrap_or_else(|_| {
                if let Some((protocol, host)) = Self::extract_host_with_protocol(&url) {
                    Self::infer_gitlab_graphql_url(&protocol, &host)
                } else {
                    format!("{}/api/graphql", url.trim_end_matches('/'))
                }
            });

            if let Ok(project_path) = std::env::var("CI_PROJECT_PATH") {
                return Some(Platform::GitLab {
                    url,
                    api_url,
                    graphql_url,
                    project_path,
                    token: None,
                });
            }
        }

        if std::env::var("GITHUB_ACTIONS").is_ok()
            && let (Ok(server_url), Ok(repository)) = (
                std::env::var("GITHUB_SERVER_URL"),
                std::env::var("GITHUB_REPOSITORY"),
            )
        {
            let url = format!("{}/{}", server_url, repository);
            let api_url = std::env::var("GITHUB_API_URL").unwrap_or_else(|_| {
                if let Some((protocol, host)) = Self::extract_host_with_protocol(&server_url) {
                    return Self::infer_github_api_url(&protocol, &host);
                }
                format!("{}/api/v3", server_url.trim_end_matches('/'))
            });

            if let Some((owner, repo)) = repository.split_once('/') {
                return Some(Platform::GitHub {
                    url,
                    api_url,
                    owner: owner.to_string(),
                    repo: repo.to_string(),
                    token: None,
                });
            }
        }

        None
    }

    fn from_origin_url(origin_url: &str) -> Self {
        match parse_git_url(origin_url) {
            Ok((host, owner, repo)) => {
                // Git URLs don't contain protocol info, so we assume HTTPS for web URLs
                let url = format!("https://{}/{}/{}", host, owner, repo);
                let protocol = "https";

                if host.contains("github") {
                    let repo_name = repo.split('/').next_back().unwrap_or(&repo);
                    Platform::GitHub {
                        url,
                        api_url: Self::infer_github_api_url(protocol, &host),
                        owner: owner.clone(),
                        repo: repo_name.to_string(),
                        token: None,
                    }
                } else if host.contains("gitlab") {
                    let project_path = format!("{}/{}", owner, repo);
                    Platform::GitLab {
                        url,
                        api_url: Self::infer_gitlab_api_url(protocol, &host),
                        graphql_url: Self::infer_gitlab_graphql_url(protocol, &host),
                        project_path,
                        token: None,
                    }
                } else {
                    Platform::Unknown
                }
            }
            Err(e) => {
                log::warn!("failed to parse git URL '{}': {}", origin_url, e);
                Platform::Unknown
            }
        }
    }

    fn infer_github_api_url(protocol: &str, host: &str) -> String {
        if host == "github.com" {
            "https://api.github.com".to_string()
        } else {
            format!("{}://{}/api/v3", protocol, host)
        }
    }

    fn infer_gitlab_api_url(protocol: &str, host: &str) -> String {
        format!("{}://{}/api/v4", protocol, host)
    }

    fn infer_gitlab_graphql_url(protocol: &str, host: &str) -> String {
        format!("{}://{}/api/graphql", protocol, host)
    }

    pub fn url(&self) -> &str {
        match self {
            Platform::GitHub { url, .. } => url,
            Platform::GitLab { url, .. } => url,
            Platform::Unknown => "",
        }
    }

    pub fn api_url(&self) -> &str {
        match self {
            Platform::GitHub { api_url, .. } => api_url,
            Platform::GitLab { api_url, .. } => api_url,
            Platform::Unknown => "",
        }
    }

    pub fn commit_url(&self, sha: &str) -> Option<String> {
        match self {
            Platform::GitHub { url, .. } => Some(format!("{}/commit/{}", url, sha)),
            Platform::GitLab { url, .. } => Some(format!("{}/-/commit/{}", url, sha)),
            Platform::Unknown => None,
        }
    }

    pub fn commits_url(
        &self,
        git_ref: &str,
        author: &str,
        since: &str,
        until: &str,
    ) -> Option<String> {
        match self {
            Platform::GitHub { url, .. } => Some(format!(
                "{}/commits/{}?author={}&since={}&until={}",
                url, git_ref, author, since, until
            )),
            _ => None,
        }
    }
}

fn parse_git_url(url: &str) -> Result<(String, String, String)> {
    let (host, path) = match url {
        s if s.starts_with("https://") => {
            let path = s.strip_prefix("https://").unwrap();
            path.split_once('/')
                .with_context(|| format!("Invalid HTTPS URL: {}", url))?
        }
        s if s.starts_with("git@") => {
            let path = s.strip_prefix("git@").unwrap();
            path.split_once(':')
                .with_context(|| format!("Invalid SSH URL: {}", url))?
        }
        _ => anyhow::bail!("URL must start with 'https://' or 'git@': {}", url),
    };

    let path = path.trim_end_matches(".git");
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match segments.as_slice() {
        [] | [_] => anyhow::bail!(
            "invalid repository path '{}'. Expected at least 'owner/repo' format",
            path
        ),
        [.., repo] => {
            let owner = segments[..segments.len() - 1].join("/");
            Ok((host.to_string(), owner, repo.to_string()))
        }
    }
}
