use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use git2::{Oid, Repository, Sort};
use semver::Version;
use serde::Serialize;

struct Tag {
    name: String,
    oid: Oid,
}

pub struct GitRepo {
    repo: Repository,
    tags: Vec<Tag>,
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

        let tags = Self::load_tags_sorted(&repo)?;

        let tag_index: HashMap<Oid, usize> = tags
            .iter()
            .enumerate()
            .map(|(idx, tag)| (tag.oid, idx))
            .collect();

        Ok(GitRepo {
            repo,
            tags,
            tag_index,
        })
    }

    fn is_semver_tag(tag_name: &str) -> bool {
        let to_parse = tag_name.strip_prefix('v').unwrap_or(tag_name);
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
        let (from_oid, from_ref) = match from {
            Some(ref from) => {
                let object = self.repo.revparse_single(from)?;
                let id = object.peel_to_commit()?.id();

                if let Some(tag) = self.tags.iter().find(|t| t.oid == id) {
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
                if let Some(&index) = self.tag_index.get(&from_oid) {
                    if index + 1 < self.tags.len() {
                        let prev_tag = &self.tags[index + 1];
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
                } else if !self.tags.is_empty() {
                    let head_oid = self.repo.head()?.peel_to_commit()?.id();

                    if from_oid == head_oid {
                        let tag = &self.tags[0];
                        (
                            Some(tag.oid),
                            Some(format!(
                                "{} ({})",
                                tag.name.clone(),
                                &tag.oid.to_string()[..7],
                            )),
                        )
                    } else if let Some(tag_oid) = self.find_closest_tag(from_oid)? {
                        let tag = self.tags.iter().find(|t| t.oid == tag_oid).unwrap();
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
            (tag: v3.0.0) To be, or not to be, that is the question
            (tag: v2.0.0) All the world's a stage
            (tag: v1.0.0) What's in a name? That which we call a rose
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(Some("v3.0.0".to_string()), None)?;

        assert_eq!(commits.len(), 1);
        assert_eq!(
            commits[0].first_line,
            "To be, or not to be, that is the question"
        );
        Ok(())
    }

    #[test]
    fn test_from_ref_is_head_with_tags() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            The course of true love never did run smooth
            Brevity is the soul of wit
            (tag: 1.0.0) Cowards die many times before their deaths
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(None, None)?;

        assert_eq!(commits.len(), 2);
        assert_eq!(
            commits[0].first_line,
            "The course of true love never did run smooth"
        );
        assert_eq!(commits[1].first_line, "Brevity is the soul of wit");
        Ok(())
    }

    #[test]
    fn test_from_ref_is_non_head_commit_with_tags() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            Some are born great, some achieve greatness
            And some have greatness thrust upon them
            (tag: v2.0.0) The lady doth protest too much, methinks
            Though this be madness, yet there is method in't
            (tag: v1.0.0) A horse! A horse! My kingdom for a horse!
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let c2_hash = test_repo.commits[1].to_string();
        let commits = git_repo.history(Some(c2_hash), None)?;

        assert_eq!(commits.len(), 1);
        assert_eq!(
            commits[0].first_line,
            "Though this be madness, yet there is method in't"
        );
        Ok(())
    }

    #[test]
    fn test_from_ref_with_no_tags() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            The better part of valor is discretion
            Lord, what fools these mortals be!
            If music be the food of love, play on
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(None, None)?;

        assert_eq!(commits.len(), 3);
        assert_eq!(
            commits[0].first_line,
            "The better part of valor is discretion"
        );
        assert_eq!(commits[1].first_line, "Lord, what fools these mortals be!");
        assert_eq!(
            commits[2].first_line,
            "If music be the food of love, play on"
        );

        Ok(())
    }

    #[test]
    fn test_auto_detection_with_non_semver_tags() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            The quality of mercy is not strained
            It droppeth as the gentle rain from heaven
            (tag: random-tag) It is twice blessed
            (tag: v1.0.0) Upon the place beneath
            Shall I compare thee to a summer's day?
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;

        // Auto-detection from HEAD should find v1.0.0 and ignore random-tag
        let commits = git_repo.history(None, None)?;

        assert_eq!(commits.len(), 3);
        assert_eq!(
            commits[0].first_line,
            "The quality of mercy is not strained"
        );
        assert_eq!(
            commits[1].first_line,
            "It droppeth as the gentle rain from heaven"
        );
        assert_eq!(commits[2].first_line, "It is twice blessed");

        Ok(())
    }
}
