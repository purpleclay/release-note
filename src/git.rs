use std::path::Path;

use anyhow::{Context, Result};
use git2::{Repository, Sort};

pub struct GitRepo {
    repo: Repository,
}

#[derive(Debug, Clone)]
pub struct Commit {
    pub hash: String,
    pub message: String,
}

impl GitRepo {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo =
            Repository::open(path).context("failed to open git repository at location: {path}")?;
        Ok(GitRepo { repo })
    }

    pub fn history(&self, from: Option<String>) -> Result<Vec<Commit>> {
        let mut commits = Vec::new();
        let mut revwalk = self
            .repo
            .revwalk()
            .context("failed to create revision walker")?;

        revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

        if let Some(from) = from {
            let object = self.repo.revparse_single(&from)?;
            revwalk.push(object.id())?;
        } else {
            revwalk.push_head()?;
        }

        for oid in revwalk {
            let commit = self
                .repo
                .find_commit(oid?)
                .context("failed to find commit")?;
            let hash = commit.id().to_string();
            let message = commit.message().unwrap_or_default().to_string();
            commits.push(Commit { hash, message });
        }
        Ok(commits)
    }
}
