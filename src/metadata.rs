use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProjectMetadata {
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub url: String,
    pub git_ref: String,
}

impl ProjectMetadata {
    pub fn new(origin_url: &str, git_ref: impl Into<String>) -> Result<Self> {
        let (host, owner, repo) = parse_git_url(origin_url)?;

        Ok(ProjectMetadata {
            url: format!("https://{}/{}/{}", host, owner, repo),
            host,
            owner,
            repo,
            git_ref: git_ref.into(),
        })
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
    match path.split('/').collect::<Vec<_>>().as_slice() {
        [owner, repo] if !owner.is_empty() && !repo.is_empty() => {
            Ok((host.to_string(), owner.to_string(), repo.to_string()))
        }
        _ => anyhow::bail!(
            "Invalid repository path '{}'. Expected 'owner/repo' format",
            path
        ),
    }
}
