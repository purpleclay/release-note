use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;

use crate::git::Commit;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
pub enum CommitCategory {
    Breaking,
    Feature,
    Fix,
    Dependencies,
    Documentation,
    CI,
    Test,
    Performance,
    Chore,
    Other,
    Refactor,
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
        if Self::is_breaking(commit) {
            return CommitCategory::Breaking;
        }

        if let Some((commit_type, scope)) = Self::parse_conventional_commit(&commit.first_line) {
            if scope == Some("deps") {
                return CommitCategory::Dependencies;
            }

            match commit_type {
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

    fn is_breaking(commit: &Commit) -> bool {
        if commit.first_line.contains("!:") {
            return true;
        }

        if let Some(footer) = &commit.footer
            && footer.starts_with("BREAKING CHANGE:")
        {
            return true;
        }

        false
    }

    fn parse_conventional_commit(first_line: &str) -> Option<(&str, Option<&str>)> {
        let re = Regex::new(r"^([a-z]+)(?:\(([a-z-]+)\))?!?:\s+.+").unwrap();

        if let Some(captures) = re.captures(first_line) {
            let commit_type = captures.get(1)?.as_str();
            let scope = captures.get(2).map(|m| m.as_str());
            Some((commit_type, scope))
        } else {
            None
        }
    }
}
