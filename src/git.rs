use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::{DiffOptions, Oid, Repository, Sort};
use regex::Regex;
use semver::Version;
use serde::Serialize;
use std::sync::LazyLock;

use crate::contributor::Contributor;

static GIT_TRAILER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z][\w-]*)\s*:\s*(.+)$").unwrap());

static LINKED_ISSUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?i)(?:close[sd]?|fix(?:es|ed)?|resolve(?:s|d)?)(?::\s*|\s+)(?:([a-zA-Z0-9_-]+)/([a-zA-Z0-9_-]+)#(\d+)|#(\d+))$"
    ).unwrap()
});

static EXCESSIVE_BLANK_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

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
        static EMAIL: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^(.+?)\s*[<(]([^>)]+)[>)]$").unwrap());

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseRange {
    pub from: String,
    pub to: Option<String>,
}

pub struct History {
    pub range: ReleaseRange,
    pub commits: Vec<Commit>,
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
    pub description: String,
    pub body: Option<String>,
    pub scope: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub breaking: bool,
    pub breaking_description: Option<String>,
    pub trailers: Vec<GitTrailer>,
    pub linked_issues: Vec<LinkedIssue>,
    pub author: String,
    pub email: String,
    pub contributors: Vec<Contributor>,
    pub timestamp: i64,
}

impl Commit {
    fn from_git2_commit(commit: &git2::Commit) -> Self {
        let hash = commit.id().to_string();
        let author = commit.author().name().unwrap_or_default().to_string();
        let email = commit.author().email().unwrap_or_default().to_string();
        let timestamp = commit.time().seconds();

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
            description: String::new(),
            body,
            scope: String::new(),
            type_: String::new(),
            breaking: false,
            breaking_description: None,
            trailers,
            linked_issues,
            author,
            email,
            contributors: Vec::new(),
            timestamp,
        }
    }

    fn normalize_blank_lines(text: &str) -> String {
        EXCESSIVE_BLANK_LINES.replace_all(text, "\n\n").to_string()
    }

    fn parse_body_and_trailers(
        lines: &[&str],
    ) -> (Option<String>, Vec<GitTrailer>, Vec<LinkedIssue>) {
        let mut linked_issues = Vec::new();
        let mut lines_to_strip = std::collections::HashSet::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if let Some(issue) = Self::extract_linked_issue_from_line(trimmed) {
                linked_issues.push(issue);
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

    fn extract_linked_issue_from_line(line: &str) -> Option<LinkedIssue> {
        LINKED_ISSUE.captures(line).and_then(|cap| {
            if let Some(num) = cap.get(3) {
                Some(LinkedIssue {
                    number: num.as_str().parse().ok()?,
                    owner: cap.get(1).map(|m| m.as_str().to_string()),
                    repo: cap.get(2).map(|m| m.as_str().to_string()),
                })
            } else if let Some(num) = cap.get(4) {
                Some(LinkedIssue {
                    number: num.as_str().parse().ok()?,
                    owner: None,
                    repo: None,
                })
            } else {
                None
            }
        })
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

        if repo.is_empty()? {
            anyhow::bail!("repository is empty and contains no commits");
        }

        if repo.is_shallow() {
            anyhow::bail!("repository is a shallow clone with incomplete history");
        }

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
            .and_then(|remote| remote.url().ok().map(|s| s.to_string()));

        Ok(GitRepo {
            repo,
            path_filter,
            origin_url,
        })
    }

    fn semver_of(tag_name: &str) -> Option<Version> {
        let version_part = tag_name.rsplit('/').next().unwrap_or(tag_name);
        let to_parse = version_part.strip_prefix('v').unwrap_or(version_part);
        Version::parse(to_parse).ok()
    }

    // Maps each tagged commit to its semver tag. When a commit carries several,
    // the highest version wins, with the tag name as a deterministic tie-break.
    fn load_semver_tags(repo: &Repository) -> Result<HashMap<Oid, String>> {
        let mut tags: HashMap<Oid, (Version, String)> = HashMap::new();
        let tag_names = repo.tag_names(None)?;

        for tag_name in tag_names.iter().flatten().flatten() {
            let Some(version) = Self::semver_of(tag_name) else {
                continue;
            };

            let tag_ref = format!("refs/tags/{}", tag_name);
            if let Ok(reference) = repo.find_reference(&tag_ref)
                && let Ok(commit) = reference.peel_to_commit()
            {
                let candidate = (version, tag_name.to_string());
                tags.entry(commit.id())
                    .and_modify(|current| {
                        if candidate > *current {
                            *current = candidate.clone();
                        }
                    })
                    .or_insert(candidate);
            }
        }

        Ok(tags
            .into_iter()
            .map(|(oid, (_, name))| (oid, name))
            .collect())
    }

    pub fn history(&self, from: Option<String>, to: Option<String>) -> Result<History> {
        let tags = Self::load_semver_tags(&self.repo)?;

        // A requested tag keeps its name; otherwise a commit is named by its
        // highest semver tag, falling back to its abbreviated hash.
        let name_for = |rev: Option<&String>, oid: Oid| -> Result<String> {
            let tag = rev
                .and_then(|rev| self.requested_tag(rev, oid))
                .or_else(|| tags.get(&oid).cloned());
            if let Some(name) = tag {
                return Ok(name);
            }
            let short_id = self.repo.find_object(oid, None)?.short_id()?;
            Ok(short_id.as_str().unwrap_or_default().to_string())
        };

        let from_oid = match &from {
            Some(rev) => self.repo.revparse_single(rev)?.peel_to_commit()?.id(),
            None => self.repo.head()?.peel_to_commit()?.id(),
        };
        let from_name = name_for(from.as_ref(), from_oid)?;

        // A tagged `from` measures from a lower version where one exists, so a
        // maintenance branch that merged main is not measured from main's release.
        let ceiling = Self::semver_of(&from_name);
        let to_oid = match &to {
            Some(rev) => Some(self.repo.revparse_single(rev)?.peel_to_commit()?.id()),
            None => match self.find_previous_tag(from_oid, &tags, ceiling.as_ref())? {
                None if ceiling.is_some() => self.find_previous_tag(from_oid, &tags, None)?,
                found => found,
            },
        };

        let range = ReleaseRange {
            from: from_name,
            to: to_oid.map(|oid| name_for(to.as_ref(), oid)).transpose()?,
        };

        log::info!(
            "scanning from {}{}",
            range.from,
            range
                .to
                .as_ref()
                .map_or_else(String::new, |to| format!(" to {to}")),
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
        Ok(History { range, commits })
    }

    // The previous release is chosen from the nearest semver-tagged ancestors of
    // `from_oid`: tagged commits in its history that no other tagged commit there
    // descends from. Tags on unmerged branches are never candidates. When merged
    // branches leave several (e.g. a backport merged back into main), the highest
    // version wins. Tags at or above `ceiling` are walked past, not considered.
    //
    // Each walk hides the tags already found and stops at the first tagged commit
    // it reaches. The walks are unsorted, which libgit2 performs lazily: both
    // topological and date sorting prepare the whole history before yielding the
    // first commit, which costs around a second on a large repository. As an
    // unsorted walk can reach an older tag before a nearer one, any candidate
    // that another candidate descends from is discarded afterwards.
    fn find_previous_tag(
        &self,
        from_oid: Oid,
        tags: &HashMap<Oid, String>,
        ceiling: Option<&Version>,
    ) -> Result<Option<Oid>> {
        let eligible = |oid: &Oid| {
            tags.get(oid).is_some_and(|name| {
                ceiling.is_none_or(|max| Self::semver_of(name).is_some_and(|v| &v < max))
            })
        };
        let mut candidates: Vec<Oid> = Vec::new();

        loop {
            let mut revwalk = self.repo.revwalk()?;
            revwalk.set_sorting(Sort::NONE)?;
            revwalk.push(from_oid)?;
            for &oid in &candidates {
                revwalk.hide(oid)?;
            }

            let mut found = None;
            for oid in revwalk {
                let oid = oid?;
                if oid != from_oid && eligible(&oid) {
                    found = Some(oid);
                    break;
                }
            }

            match found {
                Some(oid) => candidates.push(oid),
                None => break,
            }
        }

        let mut nearest = Vec::with_capacity(candidates.len());
        for &oid in &candidates {
            let mut superseded = false;
            for &other in &candidates {
                if other != oid && self.repo.graph_descendant_of(other, oid)? {
                    superseded = true;
                    break;
                }
            }
            if !superseded {
                nearest.push(oid);
            }
        }

        Ok(nearest
            .into_iter()
            .max_by_key(|oid| (Self::semver_of(&tags[oid]), tags[oid].clone())))
    }

    // The semver tag named by a `from`/`to` argument, when it points at `oid`.
    // Keeps the requested name when a commit carries several tags.
    fn requested_tag(&self, rev: &str, oid: Oid) -> Option<String> {
        let name = rev.strip_prefix("refs/tags/").unwrap_or(rev);
        Self::semver_of(name)?;
        let commit = self
            .repo
            .find_reference(&format!("refs/tags/{name}"))
            .ok()?
            .peel_to_commit()
            .ok()?;
        (commit.id() == oid).then(|| name.to_string())
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
