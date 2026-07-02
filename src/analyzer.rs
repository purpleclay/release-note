use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;

use crate::git::Commit;

static CONVENTIONAL_COMMIT_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^([a-z]+)(?:\(([a-z-]+)\))?(!)?(?:\s*):(?:\s*).+").unwrap());

static BREAKING_FOOTER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?im)^BREAKING[- ]CHANGES?:").unwrap());

static BREAKING_FOOTER_DESC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?im)^BREAKING[- ]CHANGES?:[ \t]*(?s:(.+))").unwrap());

struct ConventionalCommit {
    commit_type: String,
    scope: Option<String>,
    breaking: bool,
}

struct CommitMeta {
    scope: String,
    type_: String,
    breaking: bool,
    breaking_description: Option<String>,
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
    pub is_ai: bool,
    pub first_commit_timestamp: i64,
    pub last_commit_timestamp: i64,
}

pub struct CommitAnalyzer;

impl CommitAnalyzer {
    pub fn analyze(commits: &[Commit]) -> CategorizedCommits {
        let mut by_category: HashMap<CommitCategory, Vec<Commit>> = HashMap::new();

        for commit in commits {
            let (category, meta) = Self::categorize(commit);
            let mut c = commit.clone();
            c.scope = meta.scope;
            c.type_ = meta.type_;
            c.breaking = meta.breaking;
            c.breaking_description = meta.breaking_description;
            by_category.entry(category).or_default().push(c);
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

    fn categorize(commit: &Commit) -> (CommitCategory, CommitMeta) {
        let parsed = Self::parse_conventional_commit(&commit.first_line);
        let scope = parsed
            .as_ref()
            .and_then(|p| p.scope.clone())
            .unwrap_or_default();
        let type_ = parsed
            .as_ref()
            .map(|p| p.commit_type.clone())
            .unwrap_or_default();
        let breaking_bang = parsed.as_ref().map(|p| p.breaking).unwrap_or(false);
        let has_footer = Self::has_breaking_footer(commit);
        let breaking = breaking_bang || has_footer;
        let breaking_description = if has_footer {
            Self::extract_breaking_description(commit)
        } else {
            None
        };

        let meta = CommitMeta {
            scope,
            type_,
            breaking,
            breaking_description,
        };

        if breaking {
            return (CommitCategory::Breaking, meta);
        }

        if let Some(ref parsed) = parsed {
            if parsed.scope.as_deref() == Some("deps") {
                return (CommitCategory::Dependencies, meta);
            }

            let category = match parsed.commit_type.as_str() {
                "feat" => CommitCategory::Feature,
                "fix" => CommitCategory::Fix,
                "docs" => CommitCategory::Documentation,
                "ci" => CommitCategory::CI,
                "test" => CommitCategory::Test,
                "perf" => CommitCategory::Performance,
                "chore" => CommitCategory::Chore,
                "refactor" => CommitCategory::Refactor,
                _ => CommitCategory::Other,
            };
            (category, meta)
        } else {
            (CommitCategory::Other, meta)
        }
    }

    fn find_breaking_trailer(commit: &Commit) -> Option<&str> {
        commit.trailers.iter().find_map(|trailer| {
            if let crate::git::GitTrailer::Other { key, value } = trailer {
                let normalized = key.to_uppercase().replace('-', " ");
                if normalized == "BREAKING CHANGE" || normalized == "BREAKING CHANGES" {
                    return Some(value.as_str());
                }
            }
            None
        })
    }

    fn extract_breaking_description(commit: &Commit) -> Option<String> {
        if let Some(value) = Self::find_breaking_trailer(commit) {
            return Some(value.to_string());
        }
        if let Some(body) = &commit.body
            && let Some(caps) = BREAKING_FOOTER_DESC.captures(body)
        {
            return caps.get(1).map(|m| m.as_str().trim().to_string());
        }
        None
    }

    fn has_breaking_footer(commit: &Commit) -> bool {
        if let Some(body) = &commit.body
            && BREAKING_FOOTER.is_match(body)
        {
            return true;
        }
        Self::find_breaking_trailer(commit).is_some()
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
                    .and_modify(|summary| {
                        summary.count += 1;
                        summary.first_commit_timestamp =
                            summary.first_commit_timestamp.min(commit.timestamp);
                        summary.last_commit_timestamp =
                            summary.last_commit_timestamp.max(commit.timestamp);
                    })
                    .or_insert_with(|| ContributorSummary {
                        username: contributor.username.clone(),
                        avatar_url: contributor.avatar_url.clone(),
                        count: 1,
                        is_bot: contributor.is_bot,
                        is_ai: contributor.is_ai,
                        first_commit_timestamp: commit.timestamp,
                        last_commit_timestamp: commit.timestamp,
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
