use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::{DiffOptions, Oid, Repository, Sort};
use semver::Version;
use serde::Serialize;

struct Tag {
    name: String,
    oid: Oid,
}

pub struct GitRepo {
    repo: Repository,
    path_filter: Option<PathBuf>,
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

        Ok(GitRepo { repo, path_filter })
    }

    fn detect_prefix_from_from_ref(&self, from_ref: &Option<String>) -> Result<Option<String>> {
        let Some(ref_str) = from_ref else {
            return Ok(None);
        };

        if !ref_str.contains('/') {
            return Ok(None);
        }

        let slash_pos = ref_str.rfind('/').unwrap();
        let potential_prefix = &ref_str[..slash_pos];
        let version_part = &ref_str[slash_pos + 1..];

        let to_parse = version_part.strip_prefix('v').unwrap_or(version_part);
        if Version::parse(to_parse).is_ok() {
            Ok(Some(potential_prefix.to_string()))
        } else {
            Ok(None)
        }
    }

    fn tag_matches_prefix(tag_name: &str, prefix_filter: Option<&str>) -> bool {
        match prefix_filter {
            None => !tag_name.contains('/'),
            Some(prefix) => tag_name.starts_with(&format!("{}/", prefix)),
        }
    }

    fn is_semver_tag(tag_name: &str) -> bool {
        let version_part = tag_name.rsplit('/').next().unwrap_or(tag_name);
        let to_parse = version_part.strip_prefix('v').unwrap_or(version_part);
        Version::parse(to_parse).is_ok()
    }

    fn load_tags_sorted(repo: &Repository, prefix_filter: Option<&str>) -> Result<Vec<Tag>> {
        let mut tags = Vec::new();
        let tag_names = repo.tag_names(None)?;

        for tag_name in tag_names.iter().flatten() {
            if !Self::tag_matches_prefix(tag_name, prefix_filter) {
                continue;
            }

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
        let prefix_filter = self.detect_prefix_from_from_ref(&from)?;
        let tags = Self::load_tags_sorted(&self.repo, prefix_filter.as_deref())?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Signature, Time};
    use tempfile::TempDir;

    struct TestRepo {
        _temp_dir: TempDir,
        repo: Repository,
        pub commits: Vec<Oid>,
        commit_counter: usize,
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
                commit_counter: 0,
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

        fn write_file(&self, path: &str, content: &str) -> Result<()> {
            let file_path = self._temp_dir.path().join(path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file_path, content)?;
            Ok(())
        }

        fn commit(&mut self, message: &str) -> Result<Oid> {
            self.commit_counter += 1;
            let file_path = format!("file{}.txt", self.commit_counter);
            let content = format!("Content for commit: {}", message);
            self.write_file(&file_path, &content)?;

            let mut index = self.repo.index()?;

            if !self.commits.is_empty() {
                let parent_commit = self.repo.find_commit(*self.commits.last().unwrap())?;
                let parent_tree = parent_commit.tree()?;
                index.read_tree(&parent_tree)?;
            }

            index.add_path(Path::new(&file_path))?;
            index.write()?;

            let tree_id = index.write_tree()?;
            let tree = self.repo.find_tree(tree_id)?;

            let timestamp = 1234567890 + self.commits.len() as i64;
            let sig = Signature::new("Test User", "test@example.com", &Time::new(timestamp, 0))?;

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

        fn commit_in_path(&mut self, path: &str, message: &str) -> Result<Oid> {
            self.commit_counter += 1;
            let file_path = format!("{}/file{}.txt", path, self.commit_counter);
            let content = format!("Content for commit: {}", message);
            self.write_file(&file_path, &content)?;

            let mut index = self.repo.index()?;

            if !self.commits.is_empty() {
                let parent_commit = self.repo.find_commit(*self.commits.last().unwrap())?;
                let parent_tree = parent_commit.tree()?;
                index.read_tree(&parent_tree)?;
            }

            index.add_path(Path::new(&file_path))?;
            index.write()?;

            let tree_id = index.write_tree()?;
            let tree = self.repo.find_tree(tree_id)?;

            let timestamp = 1234567890 + self.commits.len() as i64;
            let sig = Signature::new("Test User", "test@example.com", &Time::new(timestamp, 0))?;

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

    #[test]
    fn test_monorepo_tag_filtering_with_prefix() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            (tag: search/v0.3.0) We know what we are, but know not what we may be
            (tag: ui/v0.2.0) All that glisters is not gold
            (tag: search/v0.2.0) The web of our life is of a mingled yarn
            (tag: ui/v0.1.0) Parting is such sweet sorrow
            (tag: search/v0.1.0) There is nothing either good or bad, but thinking makes it so
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(Some("search/v0.3.0".to_string()), None)?;

        assert_eq!(commits.len(), 2);
        assert_eq!(
            commits[0].first_line,
            "We know what we are, but know not what we may be"
        );
        assert_eq!(commits[1].first_line, "All that glisters is not gold");

        Ok(())
    }

    #[test]
    fn test_monorepo_mixed_tags_isolation() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            (tag: v2.0.0) When sorrows come, they come not single spies, but in battalions
            (tag: search/v0.2.0) The rest is silence
            (tag: v1.0.0) We are such stuff as dreams are made on
            (tag: search/v0.1.0) Give every man thy ear, but few thy voice
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(Some("v2.0.0".to_string()), None)?;

        assert_eq!(commits.len(), 2);
        assert_eq!(
            commits[0].first_line,
            "When sorrows come, they come not single spies, but in battalions"
        );
        assert_eq!(commits[1].first_line, "The rest is silence");

        let commits = git_repo.history(Some("search/v0.2.0".to_string()), None)?;

        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].first_line, "The rest is silence");
        assert_eq!(
            commits[1].first_line,
            "We are such stuff as dreams are made on"
        );

        Ok(())
    }

    #[test]
    fn test_monorepo_auto_detection_from_head() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            Love all, trust a few, do wrong to none
            Conscience does make cowards of us all
            (tag: search/v0.1.0) How poor are they that have not patience
            (tag: ui/v0.1.0) O, wonder! How many goodly creatures are there here
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(None, None)?;

        assert_eq!(commits.len(), 4);
        assert_eq!(
            commits[0].first_line,
            "Love all, trust a few, do wrong to none"
        );
        assert_eq!(
            commits[1].first_line,
            "Conscience does make cowards of us all"
        );
        assert_eq!(
            commits[2].first_line,
            "How poor are they that have not patience"
        );
        assert_eq!(
            commits[3].first_line,
            "O, wonder! How many goodly creatures are there here"
        );

        Ok(())
    }

    #[test]
    fn test_monorepo_multiple_slashes_in_prefix() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            (tag: component/sub/v0.2.0) What is past is prologue
            (tag: component/sub/v0.1.0) The fool doth think he is wise
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;

        let commits = git_repo.history(Some("component/sub/v0.2.0".to_string()), None)?;

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].first_line, "What is past is prologue");

        Ok(())
    }

    #[test]
    fn test_monorepo_tag_without_v_prefix() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            (tag: search/0.2.0) Be not afraid of greatness
            (tag: search/0.1.0) Some rise by sin, and some by virtue fall
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;

        let commits = git_repo.history(Some("search/0.2.0".to_string()), None)?;

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].first_line, "Be not afraid of greatness");

        Ok(())
    }

    #[test]
    fn test_monorepo_different_prefixes_isolated() -> Result<()> {
        let test_repo = TestRepo::from_log(
            "
            (tag: search/v0.3.0) Now is the winter of our discontent
            (tag: ui/v0.3.0) A rose by any other name would smell as sweet
            (tag: search/v0.2.0) Hell is empty and all the devils are here
            (tag: ui/v0.2.0) The course of true love never did run smooth
            (tag: search/v0.1.0) There are more things in heaven and earth, Horatio
            (tag: ui/v0.1.0) Good night, good night! Parting is such sweet sorrow
        ",
        )?;

        let git_repo = GitRepo::open(test_repo.path())?;
        let commits = git_repo.history(Some("search/v0.3.0".to_string()), None)?;
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].first_line, "Now is the winter of our discontent");
        assert_eq!(
            commits[1].first_line,
            "A rose by any other name would smell as sweet"
        );

        let commits = git_repo.history(Some("ui/v0.3.0".to_string()), None)?;
        assert_eq!(commits.len(), 2);
        assert_eq!(
            commits[0].first_line,
            "A rose by any other name would smell as sweet"
        );
        assert_eq!(
            commits[1].first_line,
            "Hell is empty and all the devils are here"
        );

        Ok(())
    }

    #[test]
    fn test_path_filtering_subdirectory() -> Result<()> {
        let mut test_repo = TestRepo::new()?;

        test_repo.commit("To thine own self be true")?;
        let tag1_oid = test_repo.commit("Neither a borrower nor a lender be")?;

        test_repo.commit_in_path("ui", "All the world's a stage")?;
        test_repo.commit_in_path("ui", "And all the men and women merely players")?;
        test_repo.create_tag("v1.0.0", tag1_oid)?;

        let ui_dir = test_repo.path().join("ui");
        let git_repo = GitRepo::open(&ui_dir)?;

        let commits = git_repo.history(None, None)?;
        assert_eq!(commits.len(), 2);
        assert_eq!(
            commits[0].first_line,
            "And all the men and women merely players"
        );
        assert_eq!(commits[1].first_line, "All the world's a stage");

        Ok(())
    }

    #[test]
    fn test_path_filtering_with_tag_detection() -> Result<()> {
        let mut test_repo = TestRepo::new()?;

        test_repo.commit("The fault, dear Brutus, is not in our stars")?;
        test_repo.commit_in_path("search", "But in ourselves, that we are underlings")?;
        let tag1_oid = test_repo.commit("Uneasy lies the head that wears a crown")?;
        test_repo.commit_in_path("search", "Friends, Romans, countrymen, lend me your ears")?;
        let tag2_oid =
            test_repo.commit_in_path("search", "I come to bury Caesar, not to praise him")?;
        test_repo.commit("The evil that men do lives after them")?;

        test_repo.create_tag("v1.0.0", tag1_oid)?;
        test_repo.create_tag("v2.0.0", tag2_oid)?;

        let search_dir = test_repo.path().join("search");
        let git_repo = GitRepo::open(&search_dir)?;

        let commits = git_repo.history(Some("v2.0.0".to_string()), None)?;
        assert_eq!(commits.len(), 2);
        assert_eq!(
            commits[0].first_line,
            "I come to bury Caesar, not to praise him"
        );
        assert_eq!(
            commits[1].first_line,
            "Friends, Romans, countrymen, lend me your ears"
        );

        Ok(())
    }

    #[test]
    fn test_path_filtering_nested_subdirectory() -> Result<()> {
        let mut test_repo = TestRepo::new()?;

        test_repo.commit("The readiness is all")?;
        test_repo.commit_in_path("src", "There is nothing either good or bad")?;
        test_repo.commit_in_path("src/components", "But thinking makes it so")?;
        test_repo.commit_in_path("src/components", "To be or not to be")?;
        test_repo.commit_in_path("src/utils", "That is the question")?;

        let components_dir = test_repo.path().join("src/components");
        let git_repo = GitRepo::open(&components_dir)?;

        let commits = git_repo.history(None, None)?;
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].first_line, "To be or not to be");
        assert_eq!(commits[1].first_line, "But thinking makes it so");

        Ok(())
    }
}
