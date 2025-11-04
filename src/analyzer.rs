use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;

use crate::git::Commit;

static CONVENTIONAL_COMMIT_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([a-z]+)(?:\(([a-z-]+)\))?(!)?:\s+.+").unwrap());

struct ConventionalCommit<'a> {
    commit_type: &'a str,
    scope: Option<&'a str>,
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

        for (category, commits) in &by_category {
            log::info!(
                "  * {}: {} commit{}",
                format!("{:?}", category).to_lowercase(),
                commits.len(),
                if commits.len() == 1 { "" } else { "s" }
            );
        }

        CategorizedCommits { by_category }
    }

    fn categorize(commit: &Commit) -> CommitCategory {
        if Self::has_breaking_footer(commit) {
            return CommitCategory::Breaking;
        }

        if let Some(parsed) = Self::parse_conventional_commit(&commit.first_line) {
            if parsed.breaking {
                return CommitCategory::Breaking;
            }

            if parsed.scope == Some("deps") {
                return CommitCategory::Dependencies;
            }

            match parsed.commit_type {
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
        if let Some(footer) = &commit.footer
            && footer.starts_with("BREAKING CHANGE:")
        {
            return true;
        }
        false
    }

    fn parse_conventional_commit(first_line: &str) -> Option<ConventionalCommit<'_>> {
        if let Some(captures) = CONVENTIONAL_COMMIT_PREFIX.captures(first_line) {
            let commit_type = captures.get(1)?.as_str();
            let scope = captures.get(2).map(|m| m.as_str());
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
}
