use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::{DiffOptions, Oid, Repository, Sort};
use once_cell::sync::Lazy;
use regex::Regex;
use semver::Version;
use serde::Serialize;

static GIT_TRAILER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([A-Za-z][\w-]*)\s*:\s*(.+)$").unwrap());

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
pub struct Trailer {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub hash: String,
    pub first_line: String,
    pub body: Option<String>,
    pub trailers: Vec<Trailer>,
    pub author: String,
    pub email: String,
    pub contributor: Option<String>,
}

impl Commit {
    fn from_git2_commit(commit: &git2::Commit) -> Self {
        let hash = commit.id().to_string();
        let author = commit.author().name().unwrap_or_default().to_string();
        let email = commit.author().email().unwrap_or_default().to_string();

        let message = commit.message().unwrap_or_default();
        let lines: Vec<&str> = message.lines().collect();
        let first_line = lines.first().unwrap_or(&"").to_string();

        let (body, trailers) = if lines.len() > 1 {
            Self::parse_body_and_trailers(&lines[1..])
        } else {
            (None, Vec::new())
        };

        Commit {
            hash,
            first_line,
            body,
            trailers,
            author,
            email,
            contributor: None,
        }
    }

    fn parse_body_and_trailers(lines: &[&str]) -> (Option<String>, Vec<Trailer>) {
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

        let body_lines = &lines[..trailer_start_idx];
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
            body_lines[first_non_empty..last_non_empty].join("\n")
        } else {
            String::new()
        };

        let trailers: Vec<Trailer> = lines[trailer_start_idx..]
            .iter()
            .filter_map(|line| {
                GIT_TRAILER.captures(line.trim()).map(|caps| Trailer {
                    key: caps[1].to_string(),
                    value: caps[2].trim().to_string(),
                })
            })
            .collect();

        (if body.is_empty() { None } else { Some(body) }, trailers)
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
