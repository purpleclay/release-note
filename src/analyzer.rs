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

        CategorizedCommits { by_category }
    }

    fn categorize(commit: &Commit) -> CommitCategory {
        if Self::is_breaking(commit) {
            return CommitCategory::Breaking;
        }

        if let Some((commit_type, scope)) = Self::parse_conventional_commit(&commit.first_line) {
            if scope == Some("deps") && matches!(commit_type, "fix" | "chore" | "build") {
                return CommitCategory::Dependencies;
            }

            match commit_type {
                "feat" | "refactor" => CommitCategory::Feature,
                "fix" => CommitCategory::Fix,
                "docs" => CommitCategory::Documentation,
                "ci" => CommitCategory::CI,
                "test" => CommitCategory::Test,
                "perf" => CommitCategory::Performance,
                "chore" => CommitCategory::Chore,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_commit(first_line: &str, body: Option<&str>, footer: Option<&str>) -> Commit {
        Commit {
            hash: "abc123".to_string(),
            first_line: first_line.to_string(),
            body: body.map(String::from),
            footer: footer.map(String::from),
            author: "Test Author".to_string(),
        }
    }

    #[test]
    fn test_parse_conventional_feat() {
        let result = CommitAnalyzer::parse_conventional_commit("feat: add new feature");
        assert_eq!(result, Some(("feat", None)));
    }

    #[test]
    fn test_parse_conventional_fix_with_scope() {
        let result = CommitAnalyzer::parse_conventional_commit("fix(deps): update dependency");
        assert_eq!(result, Some(("fix", Some("deps"))));
    }

    #[test]
    fn test_parse_conventional_breaking_with_bang() {
        let result = CommitAnalyzer::parse_conventional_commit("feat!: breaking change");
        assert_eq!(result, Some(("feat", None)));
    }

    #[test]
    fn test_parse_conventional_breaking_with_scope_and_bang() {
        let result = CommitAnalyzer::parse_conventional_commit("refactor(core)!: major refactor");
        assert_eq!(result, Some(("refactor", Some("core"))));
    }

    #[test]
    fn test_parse_non_conventional() {
        let result = CommitAnalyzer::parse_conventional_commit("just a regular commit message");
        assert_eq!(result, None);
    }

    #[test]
    fn test_is_breaking_with_bang() {
        let commit = create_commit("feat!: breaking change", None, None);
        assert!(CommitAnalyzer::is_breaking(&commit));
    }

    #[test]
    fn test_is_breaking_with_footer() {
        let commit = create_commit(
            "feat: some feature",
            Some("detailed description"),
            Some("BREAKING CHANGE: this breaks things"),
        );
        assert!(CommitAnalyzer::is_breaking(&commit));
    }

    #[test]
    fn test_is_not_breaking() {
        let commit = create_commit("feat: normal feature", None, None);
        assert!(!CommitAnalyzer::is_breaking(&commit));
    }

    #[test]
    fn test_categorize_feature() {
        let commit = create_commit("feat: add new feature", None, None);
        assert_eq!(CommitAnalyzer::categorize(&commit), CommitCategory::Feature);
    }

    #[test]
    fn test_categorize_refactor_as_feature() {
        let commit = create_commit("refactor: improve code", None, None);
        assert_eq!(CommitAnalyzer::categorize(&commit), CommitCategory::Feature);
    }

    #[test]
    fn test_categorize_fix() {
        let commit = create_commit("fix: resolve bug", None, None);
        assert_eq!(CommitAnalyzer::categorize(&commit), CommitCategory::Fix);
    }

    #[test]
    fn test_categorize_dependencies() {
        let commit = create_commit("fix(deps): update package", None, None);
        assert_eq!(
            CommitAnalyzer::categorize(&commit),
            CommitCategory::Dependencies
        );
    }

    #[test]
    fn test_categorize_chore_deps_as_dependencies() {
        let commit = create_commit("chore(deps): bump version", None, None);
        assert_eq!(
            CommitAnalyzer::categorize(&commit),
            CommitCategory::Dependencies
        );
    }

    #[test]
    fn test_categorize_build_deps_as_dependencies() {
        let commit = create_commit("build(deps): update build dep", None, None);
        assert_eq!(
            CommitAnalyzer::categorize(&commit),
            CommitCategory::Dependencies
        );
    }

    #[test]
    fn test_categorize_docs() {
        let commit = create_commit("docs: update README", None, None);
        assert_eq!(
            CommitAnalyzer::categorize(&commit),
            CommitCategory::Documentation
        );
    }

    #[test]
    fn test_categorize_ci() {
        let commit = create_commit("ci: update workflow", None, None);
        assert_eq!(CommitAnalyzer::categorize(&commit), CommitCategory::CI);
    }

    #[test]
    fn test_categorize_test() {
        let commit = create_commit("test: add unit tests", None, None);
        assert_eq!(CommitAnalyzer::categorize(&commit), CommitCategory::Test);
    }

    #[test]
    fn test_categorize_perf() {
        let commit = create_commit("perf: optimize algorithm", None, None);
        assert_eq!(
            CommitAnalyzer::categorize(&commit),
            CommitCategory::Performance
        );
    }

    #[test]
    fn test_categorize_chore() {
        let commit = create_commit("chore: cleanup code", None, None);
        assert_eq!(CommitAnalyzer::categorize(&commit), CommitCategory::Chore);
    }

    #[test]
    fn test_categorize_other_style() {
        let commit = create_commit("style: format code", None, None);
        assert_eq!(CommitAnalyzer::categorize(&commit), CommitCategory::Other);
    }

    #[test]
    fn test_categorize_non_conventional() {
        let commit = create_commit("just a regular commit", None, None);
        assert_eq!(CommitAnalyzer::categorize(&commit), CommitCategory::Other);
    }

    #[test]
    fn test_categorize_breaking_takes_priority() {
        let commit = create_commit("feat!: breaking feature", None, None);
        assert_eq!(
            CommitAnalyzer::categorize(&commit),
            CommitCategory::Breaking
        );
    }

    #[test]
    fn test_categorize_breaking_with_footer() {
        let commit = create_commit(
            "fix: some fix",
            Some("details"),
            Some("BREAKING CHANGE: breaks API"),
        );
        assert_eq!(
            CommitAnalyzer::categorize(&commit),
            CommitCategory::Breaking
        );
    }

    #[test]
    fn test_analyze_multiple_commits() {
        let commits = vec![
            create_commit("feat: feature 1", None, None),
            create_commit("feat: feature 2", None, None),
            create_commit("fix: bug fix", None, None),
            create_commit("docs: update docs", None, None),
            create_commit("feat!: breaking change", None, None),
        ];

        let result = CommitAnalyzer::analyze(&commits);

        assert_eq!(
            result
                .by_category
                .get(&CommitCategory::Feature)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            result.by_category.get(&CommitCategory::Fix).unwrap().len(),
            1
        );
        assert_eq!(
            result
                .by_category
                .get(&CommitCategory::Documentation)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            result
                .by_category
                .get(&CommitCategory::Breaking)
                .unwrap()
                .len(),
            1
        );
    }
}
