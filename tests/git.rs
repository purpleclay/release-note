use anyhow::Result;
use git2::{Oid, Repository, Signature, Time};
use release_note::git::GitRepo;
use std::path::Path;
use tempfile::TempDir;

const TEST_USER_NAME: &str = "William Shakespeare";
const TEST_USER_EMAIL: &str = "will@globe-theatre.com";
const BASE_TIMESTAMP: i64 = 1564567890;

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
        config.set_str("user.name", TEST_USER_NAME)?;
        config.set_str("user.email", TEST_USER_EMAIL)?;

        Ok(TestRepo {
            _temp_dir: temp_dir,
            repo,
            commits: Vec::new(),
            commit_counter: 0,
        })
    }

    fn create_signature(&self) -> Result<Signature<'static>> {
        let timestamp = BASE_TIMESTAMP + self.commits.len() as i64;
        Ok(Signature::new(
            TEST_USER_NAME,
            TEST_USER_EMAIL,
            &Time::new(timestamp, 0),
        )?)
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
                let tag_section = remaining[start + 5..start + end].trim();
                tags.extend(tag_section.split(',').map(|t| t.trim()));
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
        self.commit_internal(None, message)
    }

    fn commit_in_path(&mut self, path: &str, message: &str) -> Result<Oid> {
        self.commit_internal(Some(path), message)
    }

    fn commit_internal(&mut self, path: Option<&str>, message: &str) -> Result<Oid> {
        self.commit_counter += 1;
        let file_path = match path {
            Some(p) => format!("{}/file{}.txt", p, self.commit_counter),
            None => format!("file{}.txt", self.commit_counter),
        };
        self.write_file(&file_path, "test content")?;

        let mut index = self.repo.index()?;

        if !self.commits.is_empty() {
            let parent_oid = *self.commits.last().unwrap();
            let parent_commit = self.repo.find_commit(parent_oid)?;
            let parent_tree = parent_commit.tree()?;
            index.read_tree(&parent_tree)?;
        }

        index.add_path(Path::new(&file_path))?;
        index.write()?;

        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        let sig = self.create_signature()?;

        let parent_commit = if self.commits.is_empty() {
            None
        } else {
            let parent_oid = *self.commits.last().unwrap();
            Some(self.repo.find_commit(parent_oid)?)
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
        let sig = self.create_signature()?;

        self.repo.tag(name, commit.as_object(), &sig, "", false)?;
        Ok(())
    }

    fn path(&self) -> &std::path::Path {
        self._temp_dir.path()
    }
}

#[test]
fn includes_entire_history_on_first_release() -> Result<()> {
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
fn includes_history_between_existing_releases() -> Result<()> {
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
fn includes_history_from_head_until_first_release() -> Result<()> {
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
fn includes_history_from_commit_until_latest_release() -> Result<()> {
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
fn auto_detection_ignores_non_semver_tags() -> Result<()> {
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
fn auto_detection_supports_v_prefixed_semver_tags() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        (tag: v2.0.0) When sorrows come, they come not single spies, but in battalions
        (tag: v1.5.0) The rest is silence
        (tag: v1.0.0) We are such stuff as dreams are made on
    ",
    )?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let commits = git_repo.history(Some("v2.0.0".to_string()), None)?;

    assert_eq!(commits.len(), 1);
    assert_eq!(
        commits[0].first_line,
        "When sorrows come, they come not single spies, but in battalions"
    );

    Ok(())
}

#[test]
fn auto_detection_supports_path_prefixed_semver_tags() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        (tag: search/v0.3.0) Now is the winter of our discontent
        (tag: component/sub/v0.2.0) What is past is prologue
        (tag: search/0.2.0) Be not afraid of greatness
    ",
    )?;

    let git_repo = GitRepo::open(test_repo.path())?;

    let commits = git_repo.history(Some("component/sub/v0.2.0".to_string()), None)?;
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].first_line, "What is past is prologue");

    Ok(())
}

#[test]
fn auto_detection_only_considers_tags_at_path_within_repository() -> Result<()> {
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
fn only_includes_history_at_path_within_repository() -> Result<()> {
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

#[test]
fn detects_trailers_at_end_of_commit() -> Result<()> {
    let mut test_repo = TestRepo::new()?;

    let message = r#"feat: all the world's a stage

And all the men and women merely players.

Signed-off-by: William Shakespeare <will@globe-theatre.com>

Co-authored-by: Christopher Marlowe <kit@rose-theatre.com>

"#;
    test_repo.commit(message)?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let commits = git_repo.history(None, None)?;

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].first_line, "feat: all the world's a stage");
    assert_eq!(
        commits[0].body.as_deref(),
        Some("And all the men and women merely players.")
    );
    assert_eq!(commits[0].trailers.len(), 2);
    assert_eq!(commits[0].trailers[0].key, "Signed-off-by");
    assert_eq!(
        commits[0].trailers[0].value,
        "William Shakespeare <will@globe-theatre.com>"
    );
    assert_eq!(commits[0].trailers[1].key, "Co-authored-by");
    assert_eq!(
        commits[0].trailers[1].value,
        "Christopher Marlowe <kit@rose-theatre.com>"
    );

    Ok(())
}

#[test]
fn preserves_blank_lines_in_body() -> Result<()> {
    let mut test_repo = TestRepo::new()?;

    let message = r#"feat: to be, or not to be

That is the question: whether 'tis nobler in the mind to suffer.

The slings and arrows of outrageous fortune.

Signed-off-by: William Shakespeare <will@globe-theatre.com>"#;
    test_repo.commit(message)?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let commits = git_repo.history(None, None)?;

    assert_eq!(commits.len(), 1);
    assert_eq!(
        commits[0].body.as_deref(),
        Some(
            r#"That is the question: whether 'tis nobler in the mind to suffer.

The slings and arrows of outrageous fortune."#
        )
    );
    assert_eq!(commits[0].trailers.len(), 1);
    assert_eq!(commits[0].trailers[0].key, "Signed-off-by");

    Ok(())
}
