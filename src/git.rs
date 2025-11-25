use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::{DiffOptions, Oid, Repository, Sort};
use once_cell::sync::Lazy;
use regex::Regex;
use semver::Version;
use serde::Serialize;

use crate::contributor::Contributor;

static GIT_TRAILER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([A-Za-z][\w-]*)\s*:\s*(.+)$").unwrap());

static LINKED_ISSUE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?i)(?:close[sd]?|fix(?:es|ed)?|resolve(?:s|d)?)(?::\s*|\s+)(?:([a-zA-Z0-9_-]+)/([a-zA-Z0-9_-]+)#(\d+)|#(\d+))$"
    ).unwrap()
});

struct Tag {
    name: String,
    oid: Oid,
}

pub struct GitRepo {
    repo: Repository,
    path_filter: Option<PathBuf>,
    origin_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum GitTrailer {
    #[serde(rename_all = "kebab-case")]
    CoAuthoredBy {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    #[serde(rename_all = "kebab-case")]
    ReviewedBy {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    #[serde(rename_all = "kebab-case")]
    SignedOffBy {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    Other {
        key: String,
        value: String,
    },
}

impl GitTrailer {
    pub fn from_key_value(key: String, value: String) -> Self {
        match key.to_lowercase().as_str() {
            "co-authored-by" => Self::parse_name_email_trailer(value, |name, email| {
                GitTrailer::CoAuthoredBy { name, email }
            }),
            "reviewed-by" => Self::parse_name_email_trailer(value, |name, email| {
                GitTrailer::ReviewedBy { name, email }
            }),
            "signed-off-by" => Self::parse_name_email_trailer(value, |name, email| {
                GitTrailer::SignedOffBy { name, email }
            }),
            _ => GitTrailer::Other { key, value },
        }
    }

    fn parse_name_email_trailer<F>(value: String, constructor: F) -> Self
    where
        F: FnOnce(String, Option<String>) -> Self,
    {
        static EMAIL: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"^(.+?)\s*[<(]([^>)]+)[>)]$").unwrap());

        if let Some(caps) = EMAIL.captures(value.trim()) {
            let name = caps[1].trim().to_string();
            let email = caps[2].trim().to_string();
            constructor(
                if name.is_empty() { email.clone() } else { name },
                if email.contains('@') {
                    Some(email)
                } else {
                    None
                },
            )
        } else if value.contains('@') {
            constructor(value.clone(), Some(value))
        } else {
            constructor(value, None)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct LinkedIssue {
    pub number: u32,
    pub owner: Option<String>,
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub hash: String,
    pub first_line: String,
    pub body: Option<String>,
    pub trailers: Vec<GitTrailer>,
    pub linked_issues: Vec<LinkedIssue>,
    pub author: String,
    pub email: String,
    pub contributors: Vec<Contributor>,
}

impl Commit {
    fn from_git2_commit(commit: &git2::Commit) -> Self {
        let hash = commit.id().to_string();
        let author = commit.author().name().unwrap_or_default().to_string();
        let email = commit.author().email().unwrap_or_default().to_string();

        let message = commit.message().unwrap_or_default();
        let lines: Vec<&str> = message.lines().collect();
        let first_line = lines.first().unwrap_or(&"").to_string();

        let (body, trailers, linked_issues) = if lines.len() > 1 {
            Self::parse_body_and_trailers(&lines[1..])
        } else {
            (None, Vec::new(), Vec::new())
        };

        Commit {
            hash,
            first_line,
            body,
            trailers,
            linked_issues,
            author,
            email,
            contributors: Vec::new(),
        }
    }

    fn normalize_blank_lines(text: &str) -> String {
        let re = regex::Regex::new(r"\n{3,}").unwrap();
        re.replace_all(text, "\n\n").to_string()
    }

    fn parse_body_and_trailers(
        lines: &[&str],
    ) -> (Option<String>, Vec<GitTrailer>, Vec<LinkedIssue>) {
        let mut linked_issues = Vec::new();
        let mut lines_to_strip = std::collections::HashSet::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if LINKED_ISSUE.is_match(trimmed) {
                linked_issues.extend(Self::extract_linked_issues_from_line(trimmed));
                lines_to_strip.insert(i);
            }
        }

        let mut trailer_start_idx = lines.len();

        for (i, line) in lines.iter().enumerate().rev() {
            let trimmed = line.trim();
            if trimmed.is_empty() && i == trailer_start_idx - 1 {
                trailer_start_idx = i;
                continue;
            }

            if !trimmed.is_empty() && !GIT_TRAILER.is_match(trimmed) {
                break;
            }

            if GIT_TRAILER.is_match(trimmed) {
                trailer_start_idx = i;
            }
        }

        let body_lines: Vec<&str> = lines[..trailer_start_idx]
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                if lines_to_strip.contains(&i) {
                    None
                } else {
                    Some(*line)
                }
            })
            .collect();

        let first_non_empty = body_lines
            .iter()
            .position(|l| !l.trim().is_empty())
            .unwrap_or(0);
        let last_non_empty = body_lines
            .iter()
            .rposition(|l| !l.trim().is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);

        let body = if first_non_empty < last_non_empty {
            let joined = body_lines[first_non_empty..last_non_empty].join("\n");
            // Normalize excessive blank lines (3+ consecutive) to 2 (single paragraph break)
            Self::normalize_blank_lines(&joined)
        } else {
            String::new()
        };

        let trailers: Vec<GitTrailer> = lines[trailer_start_idx..]
            .iter()
            .filter_map(|line| {
                GIT_TRAILER.captures(line.trim()).map(|caps| {
                    GitTrailer::from_key_value(caps[1].to_string(), caps[2].trim().to_string())
                })
            })
            .collect();

        linked_issues.sort_by_key(|i| (i.owner.clone(), i.repo.clone(), i.number));
        linked_issues.dedup();

        (
            if body.is_empty() { None } else { Some(body) },
            trailers,
            linked_issues,
        )
    }

    fn extract_linked_issues_from_line(line: &str) -> Vec<LinkedIssue> {
        LINKED_ISSUE
            .captures(line)
            .map(|cap| {
                if let Some(num) = cap.get(3) {
                    vec![LinkedIssue {
                        number: num.as_str().parse().unwrap(),
                        owner: cap.get(1).map(|m| m.as_str().to_string()),
                        repo: cap.get(2).map(|m| m.as_str().to_string()),
                    }]
                } else if let Some(num) = cap.get(4) {
                    vec![LinkedIssue {
                        number: num.as_str().parse().unwrap(),
                        owner: None,
                        repo: None,
                    }]
                } else {
                    Vec::new()
                }
            })
            .unwrap_or_default()
    }
}

impl GitRepo {
    pub fn origin_url(&self) -> Option<&str> {
        self.origin_url.as_deref()
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let provided_path = path.as_ref();
        let abs_path = if provided_path.is_absolute() {
            provided_path.to_path_buf()
        } else {
            std::env::current_dir()
                .context("failed to get current directory")?
                .join(provided_path)
        };

        let repo = Repository::discover(&abs_path)
            .context("failed to find git repository from the specified location")?;

        let work_dir = repo
            .workdir()
            .context("repository has no working directory")?;

        let canonical_abs_path = abs_path.canonicalize().unwrap_or_else(|_| abs_path.clone());
        let canonical_work_dir = work_dir
            .canonicalize()
            .unwrap_or_else(|_| work_dir.to_path_buf());

        let path_filter = if canonical_abs_path.starts_with(&canonical_work_dir)
            && canonical_abs_path != canonical_work_dir
        {
            canonical_abs_path
                .strip_prefix(&canonical_work_dir)
                .ok()
                .map(|p| p.to_path_buf())
        } else {
            None
        };

        let origin_url = repo
            .find_remote("origin")
            .ok()
            .and_then(|remote| remote.url().map(|s| s.to_string()));

        Ok(GitRepo {
            repo,
            path_filter,
            origin_url,
        })
    }

    fn is_semver_tag(tag_name: &str) -> bool {
        let version_part = tag_name.rsplit('/').next().unwrap_or(tag_name);
        let to_parse = version_part.strip_prefix('v').unwrap_or(version_part);
        Version::parse(to_parse).is_ok()
    }

    fn load_tags_sorted(repo: &Repository) -> Result<Vec<Tag>> {
        let mut tags = Vec::new();
        let tag_names = repo.tag_names(None)?;

        for tag_name in tag_names.iter().flatten() {
            if !Self::is_semver_tag(tag_name) {
                continue;
            }

            let tag_ref = format!("refs/tags/{}", tag_name);
            if let Ok(reference) = repo.find_reference(&tag_ref)
                && let Ok(commit) = reference.peel_to_commit()
            {
                tags.push((tag_name.to_string(), commit.id(), commit.time().seconds()));
            }
        }

        tags.sort_by(|a, b| b.2.cmp(&a.2));
        Ok(tags
            .into_iter()
            .map(|(name, oid, _)| Tag { name, oid })
            .collect())
    }

    pub fn history(&self, from: Option<String>, to: Option<String>) -> Result<Vec<Commit>> {
        let tags = Self::load_tags_sorted(&self.repo)?;

        let tag_index: HashMap<Oid, usize> = tags
            .iter()
            .enumerate()
            .map(|(idx, tag)| (tag.oid, idx))
            .collect();

        let (from_oid, from_ref) = match from {
            Some(ref from) => {
                let object = self.repo.revparse_single(from)?;
                let id = object.peel_to_commit()?.id();

                if let Some(tag) = tags.iter().find(|t| t.oid == id) {
                    (id, format!("{} ({})", tag.name, &id.to_string()[..7]))
                } else {
                    (id, id.to_string()[..7].to_string())
                }
            }
            None => {
                let head = self.repo.head()?;
                let id = head.peel_to_commit()?.id();
                (id, format!("HEAD ({})", &id.to_string()[..7]))
            }
        };

        let (to_oid, to_ref) = match to {
            Some(ref to) => {
                let object = self.repo.revparse_single(to)?;
                let id = object.peel_to_commit()?.id();
                (Some(id), Some(id.to_string()[..7].to_string()))
            }
            None => {
                if let Some(&index) = tag_index.get(&from_oid) {
                    if index + 1 < tags.len() {
                        let prev_tag = &tags[index + 1];
                        (
                            Some(prev_tag.oid),
                            Some(format!(
                                "{} ({})",
                                prev_tag.name.clone(),
                                &prev_tag.oid.to_string()[..7],
                            )),
                        )
                    } else {
                        (None, None)
                    }
                } else if !tags.is_empty() {
                    let head_oid = self.repo.head()?.peel_to_commit()?.id();

                    if from_oid == head_oid {
                        let tag = &tags[0];
                        (
                            Some(tag.oid),
                            Some(format!(
                                "{} ({})",
                                tag.name.clone(),
                                &tag.oid.to_string()[..7],
                            )),
                        )
                    } else if let Some(tag_oid) = self.find_closest_tag(from_oid, &tag_index)? {
                        let tag = tags.iter().find(|t| t.oid == tag_oid).unwrap();
                        (
                            Some(tag.oid),
                            Some(format!(
                                "{} ({})",
                                tag.name.clone(),
                                &tag.oid.to_string()[..7],
                            )),
                        )
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            }
        };

        log::info!(
            "scanning from {}{}",
            from_ref,
            to_ref.map_or_else(|| "".to_string(), |v| format!(" to {}", v)),
        );

        if let Some(ref path) = self.path_filter {
            log::info!("filtering commits to path: {}", path.display());
        }

        let mut commits = Vec::new();
        let mut revwalk = self
            .repo
            .revwalk()
            .context("failed to create revision walker")?;

        revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        revwalk.push(from_oid)?;

        if let Some(to_oid) = to_oid {
            revwalk.hide(to_oid)?;
        }

        for oid in revwalk {
            let git_commit = self
                .repo
                .find_commit(oid?)
                .context("failed to find commit")?;

            if let Some(ref path) = self.path_filter
                && !Self::commit_touches_path(&self.repo, &git_commit, path)?
            {
                continue;
            }

            commits.push(Commit::from_git2_commit(&git_commit));
        }
        Ok(commits)
    }

    fn find_closest_tag(
        &self,
        from_oid: Oid,
        tag_index: &HashMap<Oid, usize>,
    ) -> Result<Option<Oid>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        revwalk.push(from_oid)?;

        for oid in revwalk {
            let oid = oid?;
            if tag_index.contains_key(&oid) {
                return Ok(Some(oid));
            }
        }

        Ok(None)
    }

    fn commit_touches_path(repo: &Repository, commit: &git2::Commit, path: &Path) -> Result<bool> {
        let mut path_str = path.to_string_lossy().to_string();

        if !path_str.ends_with('/') {
            path_str.push('/');
        }

        match commit.parent_count() {
            0 => {
                let tree = commit.tree()?;
                let pathspec = git2::Pathspec::new(std::iter::once(path_str.as_str()))?;
                let matches = pathspec.match_tree(&tree, git2::PathspecFlags::empty())?;
                Ok(matches.entries().count() > 0)
            }
            _ => {
                let parent = commit.parent(0)?;
                let mut diff_opts = DiffOptions::new();
                diff_opts.pathspec(&path_str);

                let diff = repo.diff_tree_to_tree(
                    Some(&parent.tree()?),
                    Some(&commit.tree()?),
                    Some(&mut diff_opts),
                )?;

                Ok(diff.deltas().count() > 0)
            }
        }
    }
}
