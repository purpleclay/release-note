use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use git2::{Oid, Repository, Sort};
use serde::Serialize;

pub struct GitRepo {
    repo: Repository,
    tag_oids: Vec<Oid>,
    tag_index: HashMap<Oid, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub hash: String,
    pub message: String,
}

impl GitRepo {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo = Repository::open(path)
            .context("failed to open git repository at the specified location")?;

        let tag_oids = Self::load_tags_sorted(&repo)?;

        let tag_index: HashMap<Oid, usize> = tag_oids
            .iter()
            .enumerate()
            .map(|(idx, &oid)| (oid, idx))
            .collect();

        Ok(GitRepo {
            repo,
            tag_oids,
            tag_index,
        })
    }

    fn load_tags_sorted(repo: &Repository) -> Result<Vec<Oid>> {
        let mut tags = Vec::new();
        let tag_names = repo.tag_names(None)?;

        for tag_name in tag_names.iter().flatten() {
            let tag_ref = format!("refs/tags/{}", tag_name);
            if let Ok(reference) = repo.find_reference(&tag_ref)
                && let Ok(commit) = reference.peel_to_commit()
            {
                tags.push((commit.id(), commit.time().seconds()));
            }
        }

        tags.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(tags.into_iter().map(|(oid, _)| oid).collect())
    }

    pub fn history(&self, from: Option<String>, to: Option<String>) -> Result<Vec<Commit>> {
        let from_ref = match from {
            Some(from) => {
                let object = self.repo.revparse_single(&from)?;
                object.peel_to_commit()?.id()
            }
            None => {
                let head = self.repo.head()?;
                head.peel_to_commit()?.id()
            }
        };

        let to_ref = match to {
            Some(to) => {
                let object = self.repo.revparse_single(&to)?;
                Some(object.peel_to_commit()?.id())
            }
            None => {
                if let Some(&index) = self.tag_index.get(&from_ref) {
                    if index + 1 < self.tag_oids.len() {
                        Some(self.tag_oids[index + 1])
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };

        let mut commits = Vec::new();
        let mut revwalk = self
            .repo
            .revwalk()
            .context("failed to create revision walker")?;

        revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        revwalk.push(from_ref)?;

        if let Some(to_ref) = to_ref {
            revwalk.hide(to_ref)?;
        }

        for oid in revwalk {
            let commit = self
                .repo
                .find_commit(oid?)
                .context("failed to find commit")?;
            let hash = commit.id().to_string();
            let message = commit
                .message()
                .unwrap_or_default()
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();
            commits.push(Commit { hash, message });
        }
        Ok(commits)
    }
}
