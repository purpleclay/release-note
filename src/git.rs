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
    pub first_line: String,
    pub body: Option<String>,
    pub footer: Option<String>,
    pub author: String,
}

impl Commit {
    fn from_git2_commit(commit: &git2::Commit) -> Self {
        let hash = commit.id().to_string();
        let author = commit.author().name().unwrap_or_default().to_string();

        let message = commit.message().unwrap_or_default();
        let lines: Vec<&str> = message.lines().collect();
        let first_line = lines.first().unwrap_or(&"").to_string();

        let (body, footer) = if lines.len() > 1 {
            let remaining = &lines[1..];
            if let Some(last_blank_idx) = remaining.iter().rposition(|line| line.trim().is_empty())
            {
                let body_lines = &remaining[..last_blank_idx];
                let footer_lines = &remaining[last_blank_idx + 1..];

                let body_text = body_lines.join("\n").trim().to_string();
                let footer_text = footer_lines.join("\n").trim().to_string();

                (
                    if body_text.is_empty() {
                        None
                    } else {
                        Some(body_text)
                    },
                    if footer_text.is_empty() {
                        None
                    } else {
                        Some(footer_text)
                    },
                )
            } else {
                let body_text = remaining.join("\n").trim().to_string();
                (
                    if body_text.is_empty() {
                        None
                    } else {
                        Some(body_text)
                    },
                    None,
                )
            }
        } else {
            (None, None)
        };

        Commit {
            hash,
            first_line,
            body,
            footer,
            author,
        }
    }
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
                } else if !self.tag_oids.is_empty() {
                    let head_oid = self.repo.head()?.peel_to_commit()?.id();

                    if from_ref == head_oid {
                        Some(self.tag_oids[0])
                    } else {
                        self.find_closest_tag(from_ref)?
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
            let git_commit = self
                .repo
                .find_commit(oid?)
                .context("failed to find commit")?;
            commits.push(Commit::from_git2_commit(&git_commit));
        }
        Ok(commits)
    }

    fn find_closest_tag(&self, from_oid: Oid) -> Result<Option<Oid>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        revwalk.push(from_oid)?;

        for oid in revwalk {
            let oid = oid?;
            if self.tag_index.contains_key(&oid) {
                return Ok(Some(oid));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Signature, Time};
    use tempfile::TempDir;

    struct TestRepo {
        _temp_dir: TempDir,
        repo: Repository,
        pub commits: Vec<Oid>,
    }

    impl TestRepo {
        fn new() -> Result<Self> {
            let temp_dir = TempDir::new()?;
            let repo = Repository::init(temp_dir.path())?;

            let mut config = repo.config()?;
            config.set_str("user.name", "Test User")?;
            config.set_str("user.email", "test@example.com")?;

            Ok(TestRepo {
                _temp_dir: temp_dir,
                repo,
                commits: Vec::new(),
            })
        }

        fn from_log(log: &str) -> Result<Self> {
            let mut test_repo = Self::new()?;
            let lines: Vec<&str> = log.trim().lines().rev().collect();

            for line in lines {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let (tags, message) = Self::parse_log_line(line);
                let oid = test_repo.commit(message)?;

                for tag in tags {
                    test_repo.create_tag(tag, oid)?;
                }
            }

            Ok(test_repo)
        }

        fn parse_log_line(line: &str) -> (Vec<&str>, &str) {
            let mut tags = Vec::new();
            let mut remaining = line;

            while let Some(start) = remaining.find("(tag:") {
                if let Some(end) = remaining[start..].find(')') {
                    let tag = remaining[start + 5..start + end].trim();
                    tags.push(tag);
                    remaining = &remaining[start + end + 1..];
                } else {
                    break;
                }
            }

            (tags, remaining.trim())
        }

        fn commit(&mut self, message: &str) -> Result<Oid> {
            let timestamp = 1234567890 + self.commits.len() as i64;
            let sig = Signature::new("Test User", "test@example.com", &Time::new(timestamp, 0))?;

            let tree = if let Some(parent_oid) = self.commits.last() {
                let parent = self.repo.find_commit(*parent_oid)?;
                parent.tree()?
            } else {
                let tree_id = self.repo.treebuilder(None)?.write()?;
                self.repo.find_tree(tree_id)?
            };

            let parent_commit = if self.commits.is_empty() {
                None
            } else {
                Some(self.repo.find_commit(*self.commits.last().unwrap())?)
            };

            let parents: Vec<_> = parent_commit.iter().collect();
            let oid = self
                .repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;

            self.commits.push(oid);
            Ok(oid)
        }

        fn create_tag(&self, name: &str, commit_oid: Oid) -> Result<()> {
            let commit = self.repo.find_commit(commit_oid)?;
            let sig = Signature::new("Test User", "test@example.com", &Time::new(1234567890, 0))?;

            self.repo.tag(
                name,
                commit.as_object(),
                &sig,
                &format!("Tag {}", name),
                false,
            )?;
            Ok(())
        }

        fn path(&self) -> &std::path::Path {
            self._temp_dir.path()
        }
    }

    #[test]
    fn test_from_ref_is_tag() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            (tag: v3.0.0) Third commit
            (tag: v2.0.0) Second commit
            (tag: v1.0.0) First commit
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(Some("v3.0.0".to_string()), None)?;

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].first_line, "Third commit");
        Ok(())
    }

    #[test]
    fn test_from_ref_is_head_with_tags() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            Third commit
            Second commit
            (tag: v1.0.0) First commit
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(None, None)?;

        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].first_line, "Third commit");
        assert_eq!(commits[1].first_line, "Second commit");
        Ok(())
    }

    #[test]
    fn test_from_ref_is_non_head_commit_with_tags() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            Fifth commit
            Fourth commit
            (tag: v2.0.0) Third commit
            Second commit
            (tag: v1.0.0) First commit
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let c2_hash = test_repo.commits[1].to_string();
        let commits = git_repo.history(Some(c2_hash), None)?;

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].first_line, "Second commit");
        Ok(())
    }

    #[test]
    fn test_from_ref_with_no_tags() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            Third commit
            Second commit
            First commit
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(None, None)?;

        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].first_line, "Third commit");
        assert_eq!(commits[1].first_line, "Second commit");
        assert_eq!(commits[2].first_line, "First commit");

        Ok(())
    }
}
