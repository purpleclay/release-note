use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

use crate::git::Commit;

static CONVENTIONAL_COMMIT_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^([a-z]+)(?:\(([a-z-]+)\))?(!)?(?:\s*):(?:\s*)(.+)").unwrap());

static BREAKING_FOOTER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?im)^BREAKING[- ]CHANGES?:").unwrap());

static BREAKING_FOOTER_DESC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?im)^BREAKING[- ]CHANGES?:[ \t]*(?s:(.+))").unwrap());

struct ConventionalCommit {
    commit_type: String,
    scope: Option<String>,
    breaking: bool,
    description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzedCommits {
    pub commits: Vec<Commit>,
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
    pub fn analyze(commits: &[Commit]) -> AnalyzedCommits {
        let analyzed: Vec<Commit> = commits.iter().map(Self::analyze_commit).collect();

        let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
        for commit in &analyzed {
            let type_ = if commit.type_.is_empty() {
                "other"
            } else {
                commit.type_.as_str()
            };
            *by_type.entry(type_).or_default() += 1;
        }

        log::info!("analyzed commits by type");
        for (type_, count) in &by_type {
            log::info!(
                "  * {}: {} commit{}",
                type_,
                count,
                if *count == 1 { "" } else { "s" }
            );
        }

        let contributors = Self::aggregate_contributors(commits);

        AnalyzedCommits {
            commits: analyzed,
            contributors,
        }
    }

    fn analyze_commit(commit: &Commit) -> Commit {
        let parsed = Self::parse_conventional_commit(&commit.first_line);
        let has_footer = Self::has_breaking_footer(commit);

        let mut c = commit.clone();
        c.breaking = parsed.as_ref().is_some_and(|p| p.breaking) || has_footer;
        c.breaking_description = if has_footer {
            Self::extract_breaking_description(commit)
        } else {
            None
        };

        if let Some(parsed) = parsed {
            c.description = parsed.description;
            c.scope = parsed.scope.unwrap_or_default();
            c.type_ = parsed.commit_type;
        } else {
            c.description = commit.first_line.clone();
        }

        c
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
            let description = captures.get(4)?.as_str().to_string();

            Some(ConventionalCommit {
                commit_type,
                scope,
                breaking,
                description,
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
