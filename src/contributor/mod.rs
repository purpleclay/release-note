mod github;
mod gitlab;

pub use github::GitHubResolver;
pub use gitlab::GitLabResolver;

use anyhow::Result;
use serde::Serialize;

use crate::git::Commit;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct Contributor {
    pub username: String,
    pub avatar_url: String,
    pub is_bot: bool,
}

pub trait PlatformResolver {
    fn resolve(&mut self, commit_hash: &str, email: &str) -> Option<Contributor>;

    /// Resolves known AI assistant contributors by their email addresses.
    ///
    /// This is a default implementation that can be overridden by specific platforms
    /// if they have custom AI contributor detection logic.
    ///
    /// Currently supported:
    /// - Claude: Uses `noreply@anthropic.com` as documented in Claude Code
    ///   (See: https://github.com/anthropics/claude-code/issues/1653)
    fn resolve_ai_contributor(email: &str) -> Option<String>
    where
        Self: Sized,
    {
        use once_cell::sync::Lazy;
        use std::collections::HashMap;

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
        } else if url.contains("gitlab.com") {
            log::info!("project is hosted on GitLab (gitlab.com)");
            Ok(Some(Self {
                platform_resolver: Box::new(GitLabResolver::new(url)?),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_github_platform_from_https_url() {
        let result = ContributorResolver::from_url("https://github.com/owner/repo.git");
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn detects_github_platform_from_ssh_url() {
        let result = ContributorResolver::from_url("git@github.com:owner/repo.git");
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn detects_gitlab_platform_from_https_url() {
        let result = ContributorResolver::from_url("https://gitlab.com/owner/repo.git");
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn detects_gitlab_platform_from_ssh_url() {
        let result = ContributorResolver::from_url("git@gitlab.com:owner/repo.git");
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn returns_none_for_self_hosted_git() {
        let result = ContributorResolver::from_url("https://git.mycompany.com/owner/repo.git");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
