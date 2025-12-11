use release_note::platform::Platform;
use std::env;

struct EnvVars {
    keys: Vec<String>,
}

impl EnvVars {
    fn set(vars: &[(&str, &str)]) -> Self {
        // First clear all CI environment variables to ensure clean state
        // This is critical when running tests in CI environments like GitHub Actions
        let ci_vars = [
            "GITHUB_ACTIONS",
            "GITHUB_SERVER_URL",
            "GITHUB_API_URL",
            "GITHUB_REPOSITORY",
            "GITHUB_TOKEN",
            "GITLAB_CI",
            "CI_PROJECT_URL",
            "CI_API_V4_URL",
            "CI_API_GRAPHQL_URL",
            "CI_PROJECT_PATH",
            "GITLAB_TOKEN",
        ];

        unsafe {
            for key in &ci_vars {
                env::remove_var(key);
            }
            for (key, value) in vars {
                env::set_var(key, value);
            }
        }

        // Track both cleared and set variables for restoration
        let mut all_keys: Vec<String> = ci_vars.iter().map(|k| k.to_string()).collect();
        all_keys.extend(vars.iter().map(|(k, _)| k.to_string()));
        all_keys.dedup();

        EnvVars { keys: all_keys }
    }

    fn clear_ci_env() -> Self {
        Self::set(&[])
    }
}

impl Drop for EnvVars {
    fn drop(&mut self) {
        unsafe {
            for key in &self.keys {
                env::remove_var(key);
            }
        }
    }
}

#[test]
fn detects_github_from_https_url() {
    let _clean_env = EnvVars::clear_ci_env();

    assert_eq!(
        Platform::detect(Some("https://github.com/owner/repo.git")),
        Platform::GitHub {
            url: "https://github.com/owner/repo".to_string(),
            api_url: "https://api.github.com".to_string(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_github_from_ssh_url() {
    let _clean_env = EnvVars::clear_ci_env();

    assert_eq!(
        Platform::detect(Some("git@github.com:owner/repo.git")),
        Platform::GitHub {
            url: "https://github.com/owner/repo".to_string(),
            api_url: "https://api.github.com".to_string(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_gitlab_from_https_url() {
    let _clean_env = EnvVars::clear_ci_env();

    assert_eq!(
        Platform::detect(Some("https://gitlab.com/owner/group/repo.git")),
        Platform::GitLab {
            url: "https://gitlab.com/owner/group/repo".to_string(),
            api_url: "https://gitlab.com/api/v4".to_string(),
            graphql_url: "https://gitlab.com/api/graphql".to_string(),
            project_path: "owner/group/repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_gitlab_from_ssh_url() {
    let _clean_env = EnvVars::clear_ci_env();

    assert_eq!(
        Platform::detect(Some("git@gitlab.com:owner/group/repo.git")),
        Platform::GitLab {
            url: "https://gitlab.com/owner/group/repo".to_string(),
            api_url: "https://gitlab.com/api/v4".to_string(),
            graphql_url: "https://gitlab.com/api/graphql".to_string(),
            project_path: "owner/group/repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_github_enterprise_from_https_url() {
    let _clean_env = EnvVars::clear_ci_env();

    assert_eq!(
        Platform::detect(Some("https://github.company.com/owner/repo.git")),
        Platform::GitHub {
            url: "https://github.company.com/owner/repo".to_string(),
            api_url: "https://github.company.com/api/v3".to_string(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_self_hosted_gitlab_from_https_url() {
    let _clean_env = EnvVars::clear_ci_env();

    assert_eq!(
        Platform::detect(Some("https://gitlab.company.com/owner/repo.git")),
        Platform::GitLab {
            url: "https://gitlab.company.com/owner/repo".to_string(),
            api_url: "https://gitlab.company.com/api/v4".to_string(),
            graphql_url: "https://gitlab.company.com/api/graphql".to_string(),
            project_path: "owner/repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_unknown_for_unrecognized_host() {
    let _clean_env = EnvVars::clear_ci_env();

    assert_eq!(
        Platform::detect(Some("https://git.company.com/owner/repo.git")),
        Platform::Unknown
    );
}

#[test]
fn detects_unknown_when_no_origin_url() {
    let _clean_env = EnvVars::clear_ci_env();

    assert_eq!(Platform::detect(None), Platform::Unknown);
}

#[test]
fn detects_github_from_actions_env() {
    let _env = EnvVars::set(&[
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_SERVER_URL", "https://github.com"),
        ("GITHUB_REPOSITORY", "owner/repo"),
    ]);

    assert_eq!(
        Platform::detect(None),
        Platform::GitHub {
            url: "https://github.com/owner/repo".to_string(),
            api_url: "https://api.github.com".to_string(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_github_enterprise_from_actions_env() {
    let _env = EnvVars::set(&[
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_SERVER_URL", "https://github.company.com"),
        ("GITHUB_REPOSITORY", "owner/repo"),
    ]);

    assert_eq!(
        Platform::detect(None),
        Platform::GitHub {
            url: "https://github.company.com/owner/repo".to_string(),
            api_url: "https://github.company.com/api/v3".to_string(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_github_with_custom_api_url() {
    let _env = EnvVars::set(&[
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_SERVER_URL", "https://github.company.com"),
        ("GITHUB_API_URL", "https://api.github.company.com"),
        ("GITHUB_REPOSITORY", "owner/repo"),
    ]);

    assert_eq!(
        Platform::detect(None),
        Platform::GitHub {
            url: "https://github.company.com/owner/repo".to_string(),
            api_url: "https://api.github.company.com".to_string(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_gitlab_from_ci_env() {
    let _env = EnvVars::set(&[
        ("GITLAB_CI", "true"),
        ("CI_PROJECT_URL", "https://gitlab.com/owner/repo"),
        ("CI_PROJECT_PATH", "owner/repo"),
    ]);

    assert_eq!(
        Platform::detect(None),
        Platform::GitLab {
            url: "https://gitlab.com/owner/repo".to_string(),
            api_url: "https://gitlab.com/api/v4".to_string(),
            graphql_url: "https://gitlab.com/api/graphql".to_string(),
            project_path: "owner/repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_gitlab_with_nested_groups() {
    let _env = EnvVars::set(&[
        ("GITLAB_CI", "true"),
        (
            "CI_PROJECT_URL",
            "https://gitlab.com/owner/group/subgroup/repo",
        ),
        ("CI_PROJECT_PATH", "owner/group/subgroup/repo"),
    ]);

    assert_eq!(
        Platform::detect(None),
        Platform::GitLab {
            url: "https://gitlab.com/owner/group/subgroup/repo".to_string(),
            api_url: "https://gitlab.com/api/v4".to_string(),
            graphql_url: "https://gitlab.com/api/graphql".to_string(),
            project_path: "owner/group/subgroup/repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_self_hosted_gitlab_from_ci_env() {
    let _env = EnvVars::set(&[
        ("GITLAB_CI", "true"),
        ("CI_PROJECT_URL", "https://gitlab.company.com/owner/repo"),
        ("CI_PROJECT_PATH", "owner/repo"),
    ]);

    assert_eq!(
        Platform::detect(None),
        Platform::GitLab {
            url: "https://gitlab.company.com/owner/repo".to_string(),
            api_url: "https://gitlab.company.com/api/v4".to_string(),
            graphql_url: "https://gitlab.company.com/api/graphql".to_string(),
            project_path: "owner/repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_gitlab_with_custom_api_urls() {
    let _env = EnvVars::set(&[
        ("GITLAB_CI", "true"),
        ("CI_PROJECT_URL", "https://gitlab.company.com/owner/repo"),
        ("CI_API_V4_URL", "https://api.gitlab.company.com/v4"),
        (
            "CI_API_GRAPHQL_URL",
            "https://api.gitlab.company.com/graphql",
        ),
        ("CI_PROJECT_PATH", "owner/repo"),
    ]);

    assert_eq!(
        Platform::detect(None),
        Platform::GitLab {
            url: "https://gitlab.company.com/owner/repo".to_string(),
            api_url: "https://api.gitlab.company.com/v4".to_string(),
            graphql_url: "https://api.gitlab.company.com/graphql".to_string(),
            project_path: "owner/repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn ci_detection_takes_precedence_over_url() {
    let _env = EnvVars::set(&[
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_SERVER_URL", "https://github.com"),
        ("GITHUB_REPOSITORY", "ci-owner/ci-repo"),
    ]);

    assert_eq!(
        Platform::detect(Some("https://gitlab.com/url-owner/url-repo.git")),
        Platform::GitHub {
            url: "https://github.com/ci-owner/ci-repo".to_string(),
            api_url: "https://api.github.com".to_string(),
            owner: "ci-owner".to_string(),
            repo: "ci-repo".to_string(),
            token: None,
        }
    );
}

#[test]
fn detects_github_token_from_env() {
    let _env = EnvVars::set(&[
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_SERVER_URL", "https://github.com"),
        ("GITHUB_REPOSITORY", "owner/repo"),
        ("GITHUB_TOKEN", "ghp_test_token_123"),
    ]);

    assert_eq!(
        Platform::detect(None),
        Platform::GitHub {
            url: "https://github.com/owner/repo".to_string(),
            api_url: "https://api.github.com".to_string(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            token: Some("ghp_test_token_123".to_string()),
        }
    );
}

#[test]
fn detects_gitlab_token_from_env() {
    let _env = EnvVars::set(&[
        ("GITLAB_CI", "true"),
        ("CI_PROJECT_URL", "https://gitlab.com/owner/repo"),
        ("CI_PROJECT_PATH", "owner/repo"),
        ("GITLAB_TOKEN", "glpat_test_token_456"),
    ]);

    assert_eq!(
        Platform::detect(None),
        Platform::GitLab {
            url: "https://gitlab.com/owner/repo".to_string(),
            api_url: "https://gitlab.com/api/v4".to_string(),
            graphql_url: "https://gitlab.com/api/graphql".to_string(),
            project_path: "owner/repo".to_string(),
            token: Some("glpat_test_token_456".to_string()),
        }
    );
}
