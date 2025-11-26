use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;

use crate::git::Commit;

static CONVENTIONAL_COMMIT_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^([a-z]+)(?:\(([a-z-]+)\))?(!)?(?:\s*):(?:\s*).+").unwrap());

static BREAKING_FOOTER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?im)^BREAKING[- ]CHANGES?:").unwrap());

struct ConventionalCommit {
    commit_type: String,
    scope: Option<String>,
    breaking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
pub enum CommitCategory {
    Breaking,
    Chore,
    CI,
    Dependencies,
    Documentation,
    Feature,
    Fix,
    Other,
    Performance,
    Refactor,
    Test,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategorizedCommits {
    pub by_category: HashMap<CommitCategory, Vec<Commit>>,
    pub contributors: Vec<ContributorSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContributorSummary {
    pub username: String,
    pub avatar_url: String,
    pub count: usize,
    pub is_bot: bool,
}

pub struct CommitAnalyzer;

impl CommitAnalyzer {
    pub fn analyze(commits: &[Commit]) -> CategorizedCommits {
        let mut by_category: HashMap<CommitCategory, Vec<Commit>> = HashMap::new();

        for commit in commits {
            let category = Self::categorize(commit);
            by_category
                .entry(category)
                .or_default()
                .push(commit.clone());
        }

        log::info!("attempting to categorize commits");
        for (category, commits) in &by_category {
            log::info!(
                "  * {}: {} commit{}",
                format!("{:?}", category).to_lowercase(),
                commits.len(),
                if commits.len() == 1 { "" } else { "s" }
            );
        }

        let contributors = Self::aggregate_contributors(commits);

        CategorizedCommits {
            by_category,
            contributors,
        }
    }

    fn categorize(commit: &Commit) -> CommitCategory {
        if Self::has_breaking_footer(commit) {
            return CommitCategory::Breaking;
        }

        if let Some(parsed) = Self::parse_conventional_commit(&commit.first_line) {
            if parsed.breaking {
                return CommitCategory::Breaking;
            }

            if parsed.scope.as_deref() == Some("deps") {
                return CommitCategory::Dependencies;
            }

            match parsed.commit_type.as_str() {
                "feat" => CommitCategory::Feature,
                "fix" => CommitCategory::Fix,
                "docs" => CommitCategory::Documentation,
                "ci" => CommitCategory::CI,
                "test" => CommitCategory::Test,
                "perf" => CommitCategory::Performance,
                "chore" => CommitCategory::Chore,
                "refactor" => CommitCategory::Refactor,
                _ => CommitCategory::Other,
            }
        } else {
            CommitCategory::Other
        }
    }

    fn has_breaking_footer(commit: &Commit) -> bool {
        if let Some(body) = &commit.body
            && BREAKING_FOOTER.is_match(body)
        {
            return true;
        }

        for trailer in &commit.trailers {
            if let crate::git::GitTrailer::Other { key, .. } = trailer {
                let normalized = key.to_uppercase().replace('-', " ");
                if normalized == "BREAKING CHANGE" || normalized == "BREAKING CHANGES" {
                    return true;
                }
            }
        }

        false
    }

    fn parse_conventional_commit(first_line: &str) -> Option<ConventionalCommit> {
        if let Some(captures) = CONVENTIONAL_COMMIT_PREFIX.captures(first_line) {
            let commit_type = captures.get(1)?.as_str().to_lowercase();
            let scope = captures.get(2).map(|m| m.as_str().to_lowercase());
            let breaking = captures.get(3).is_some();

            Some(ConventionalCommit {
                commit_type,
                scope,
                breaking,
            })
        } else {
            None
        }
    }

    fn aggregate_contributors(commits: &[Commit]) -> Vec<ContributorSummary> {
        let mut contributor_map: HashMap<String, ContributorSummary> = HashMap::new();

        for commit in commits {
            for contributor in &commit.contributors {
                contributor_map
                    .entry(contributor.username.clone())
                    .and_modify(|summary| summary.count += 1)
                    .or_insert_with(|| ContributorSummary {
                        username: contributor.username.clone(),
                        avatar_url: contributor.avatar_url.clone(),
                        count: 1,
                        is_bot: contributor.is_bot,
                    });
            }
        }

        let mut contributors: Vec<_> = contributor_map.into_values().collect();
        contributors.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.username.cmp(&b.username))
        });

        contributors
    }
}
