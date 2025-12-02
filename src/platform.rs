use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    GitHub,
    GitLab,
    Unknown,
}

impl Platform {
    pub fn detect(url: &str) -> Self {
        if url.contains("github.com") {
            Platform::GitHub
        } else if url.contains("gitlab.com") {
            Platform::GitLab
        } else {
            Platform::Unknown
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
            Platform::GitHub
        );
    }

    #[test]
    fn detects_github_from_ssh_url() {
        assert_eq!(
            Platform::detect("git@github.com:owner/repo.git"),
            Platform::GitHub
        );
    }

    #[test]
    fn detects_gitlab_from_https_url() {
        assert_eq!(
            Platform::detect("https://gitlab.com/owner/repo.git"),
            Platform::GitLab
        );
    }

    #[test]
    fn detects_gitlab_from_ssh_url() {
        assert_eq!(
            Platform::detect("git@gitlab.com:owner/repo.git"),
            Platform::GitLab
        );
    }

    #[test]
    fn detects_unknown_for_self_hosted() {
        assert_eq!(
            Platform::detect("https://git.company.com/owner/repo.git"),
            Platform::Unknown
        );
    }
}
