use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    GitHub(String),
    GitLab(String),
    Unknown(String),
}

impl Platform {
    pub fn detect(url: &str) -> Self {
        match parse_git_url(url) {
            Ok((host, owner, repo)) => {
                let base_url = format!("https://{}/{}/{}", host, owner, repo);

                if base_url.contains("github.com") {
                    Platform::GitHub(base_url)
                } else if base_url.contains("gitlab.com") {
                    Platform::GitLab(base_url)
                } else {
                    Platform::Unknown(base_url)
                }
            }
            Err(e) => {
                log::warn!("failed to parse git URL '{}': {}", url, e);
                Platform::Unknown(url.to_string())
            }
        }
    }

    pub fn url(&self) -> &str {
        match self {
            Platform::GitHub(url) | Platform::GitLab(url) | Platform::Unknown(url) => url,
        }
    }

    pub fn commit_url(&self, sha: &str) -> Option<String> {
        match self {
            Platform::GitHub(url) => Some(format!("{}/commit/{}", url, sha)),
            Platform::GitLab(url) => Some(format!("{}/-/commit/{}", url, sha)),
            _ => None,
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
            Platform::GitHub(url) => Some(format!(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_github_from_https_url() {
        assert_eq!(
            Platform::detect("https://github.com/owner/repo.git"),
            Platform::GitHub("https://github.com/owner/repo".to_string())
        );
    }

    #[test]
    fn detects_github_from_ssh_url() {
        assert_eq!(
            Platform::detect("git@github.com:owner/repo.git"),
            Platform::GitHub("https://github.com/owner/repo".to_string())
        );
    }

    #[test]
    fn detects_gitlab_from_https_url() {
        assert_eq!(
            Platform::detect("https://gitlab.com/owner/group/repo.git"),
            Platform::GitLab("https://gitlab.com/owner/group/repo".to_string())
        );
    }

    #[test]
    fn detects_gitlab_from_ssh_url() {
        assert_eq!(
            Platform::detect("git@gitlab.com:owner/group/repo.git"),
            Platform::GitLab("https://gitlab.com/owner/group/repo".to_string())
        );
    }

    #[test]
    fn detects_unknown_for_self_hosted() {
        assert_eq!(
            Platform::detect("https://git.company.com/owner/repo.git"),
            Platform::Unknown("https://git.company.com/owner/repo".to_string())
        );
    }
}
