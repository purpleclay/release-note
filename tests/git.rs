use anyhow::Result;
use git2::{ObjectType, Oid, Repository, Signature, Time};
use release_note::git::{GitRepo, GitTrailer};
use std::collections::HashMap;
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

    fn commit_on(&mut self, parents: &[Oid], message: &str, timestamp: i64) -> Result<Oid> {
        self.commit_counter += 1;
        let file_path = format!("file{}.txt", self.commit_counter);
        self.write_file(&file_path, "test content")?;

        let parent_commits = parents
            .iter()
            .map(|oid| self.repo.find_commit(*oid))
            .collect::<Result<Vec<_>, _>>()?;
        let mut index = self.repo.index()?;
        index.read_tree(&parent_commits[0].tree()?)?;
        index.add_path(Path::new(&file_path))?;
        let tree = self.repo.find_tree(index.write_tree()?)?;

        let sig = Signature::new(TEST_USER_NAME, TEST_USER_EMAIL, &Time::new(timestamp, 0))?;
        let parents: Vec<_> = parent_commits.iter().collect();
        Ok(self
            .repo
            .commit(None, &sig, &sig, message, &tree, &parents)?)
    }

    fn merge(&mut self, other: Oid, message: &str) -> Result<Oid> {
        let head = self.repo.find_commit(*self.commits.last().unwrap())?;
        let other = self.repo.find_commit(other)?;
        let tree = head.tree()?;

        let sig = self.create_signature()?;
        let oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head, &other])?;
        self.commits.push(oid);
        Ok(oid)
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
    let commits = git_repo.history(None, None)?.commits;

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
fn fails_on_empty_repository() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let _repo = Repository::init(temp_dir.path())?;

    let result = GitRepo::open(temp_dir.path());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("empty"));

    Ok(())
}

#[test]
fn fails_on_shallow_clone() -> Result<()> {
    let test_repo = TestRepo::from_log(
        r#"
        feat: we know what we are, but know not what we may be
        fix: some are born great, some achieve greatness
        "#,
    )?;

    let shallow_file = test_repo.repo.path().join("shallow");
    std::fs::write(&shallow_file, format!("{}\n", test_repo.commits[0]))?;

    let result = GitRepo::open(test_repo.path());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("shallow"));

    Ok(())
}

#[test]
fn extracts_and_strips_linked_issues() -> Result<()> {
    let mut test_repo = TestRepo::new()?;

    let message = r#"feat: to thine own self be true
closes #1
CLOSE: #2
closed #3
fix #4
fixes: #5
FIXED #6
resolve #7
RESOLVED #8
resolves: #9
This above all: to thine own self be true, and it must follow, as the night the day.
Resolves globe-theatre/hamlet#100
Thou canst not then be false to any man."#;
    test_repo.commit(message)?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let commits = git_repo.history(None, None)?.commits;

    assert_eq!(commits.len(), 1);
    assert_eq!(
        commits[0].body.as_deref(),
        Some(
            "This above all: to thine own self be true, and it must follow, as the night the day.\nThou canst not then be false to any man."
        )
    );

    assert_eq!(commits[0].linked_issues.len(), 10);
    assert_eq!(commits[0].linked_issues[0].number, 1);
    assert_eq!(commits[0].linked_issues[0].owner, None);
    assert_eq!(commits[0].linked_issues[0].repo, None);
    assert_eq!(commits[0].linked_issues[1].number, 2);
    assert_eq!(commits[0].linked_issues[2].number, 3);
    assert_eq!(commits[0].linked_issues[3].number, 4);
    assert_eq!(commits[0].linked_issues[4].number, 5);
    assert_eq!(commits[0].linked_issues[5].number, 6);
    assert_eq!(commits[0].linked_issues[6].number, 7);
    assert_eq!(commits[0].linked_issues[7].number, 8);
    assert_eq!(commits[0].linked_issues[8].number, 9);
    assert_eq!(commits[0].linked_issues[9].number, 100);
    assert_eq!(
        commits[0].linked_issues[9].owner.as_deref(),
        Some("globe-theatre")
    );
    assert_eq!(commits[0].linked_issues[9].repo.as_deref(), Some("hamlet"));

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
    let commits = git_repo.history(Some("v3.0.0".to_string()), None)?.commits;

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
    let commits = git_repo.history(None, None)?.commits;

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
    let commits = git_repo.history(Some(c2_hash), None)?.commits;

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
    let commits = git_repo.history(None, None)?.commits;

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
    let commits = git_repo.history(Some("v2.0.0".to_string()), None)?.commits;

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

    let commits = git_repo
        .history(Some("component/sub/v0.2.0".to_string()), None)?
        .commits;
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

    let commits = git_repo.history(Some("v2.0.0".to_string()), None)?.commits;
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

    let commits = git_repo.history(None, None)?.commits;
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
    let commits = git_repo.history(None, None)?.commits;

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].first_line, "feat: all the world's a stage");
    assert_eq!(
        commits[0].body.as_deref(),
        Some("And all the men and women merely players.")
    );
    assert_eq!(commits[0].trailers.len(), 2);
    match &commits[0].trailers[0] {
        GitTrailer::SignedOffBy { name, email } => {
            assert_eq!(name, "William Shakespeare");
            assert_eq!(email.as_deref(), Some("will@globe-theatre.com"));
        }
        _ => panic!("Expected SignedOffBy trailer"),
    }
    match &commits[0].trailers[1] {
        GitTrailer::CoAuthoredBy { name, email } => {
            assert_eq!(name, "Christopher Marlowe");
            assert_eq!(email.as_deref(), Some("kit@rose-theatre.com"));
        }
        _ => panic!("Expected CoAuthoredBy trailer"),
    }

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
    let commits = git_repo.history(None, None)?.commits;

    assert_eq!(commits.len(), 1);
    assert_eq!(
        commits[0].body.as_deref(),
        Some(
            r#"That is the question: whether 'tis nobler in the mind to suffer.

The slings and arrows of outrageous fortune."#
        )
    );
    assert_eq!(commits[0].trailers.len(), 1);
    match &commits[0].trailers[0] {
        GitTrailer::SignedOffBy { name, email } => {
            assert_eq!(name, "William Shakespeare");
            assert_eq!(email.as_deref(), Some("will@globe-theatre.com"));
        }
        _ => panic!("Expected SignedOffBy trailer"),
    }

    Ok(())
}

#[test]
fn strips_linked_issues_and_normalizes_blank_lines() -> Result<()> {
    let mut test_repo = TestRepo::new()?;

    let message = r#"feat: introduce the play within a play

We'll have a play extempore. The play's the thing wherein I'll catch
the conscience of the king.


Closes #42
Fixes owner/repo#108
Resolves #256


This mechanism allows for the revelation of truth through theatrical
performance, mirroring reality back to the audience.

Signed-off-by: William Shakespeare <will@globe-theatre.com>
Co-authored-by: Christopher Marlowe <kit@rose-theatre.com>"#;
    test_repo.commit(message)?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let commits = git_repo.history(None, None)?.commits;

    assert_eq!(commits.len(), 1);

    assert_eq!(commits[0].linked_issues.len(), 3);
    assert_eq!(commits[0].linked_issues[0].number, 42);
    assert_eq!(commits[0].linked_issues[0].owner, None);
    assert_eq!(commits[0].linked_issues[1].number, 256);
    assert_eq!(commits[0].linked_issues[1].owner, None);
    assert_eq!(commits[0].linked_issues[2].number, 108);
    assert_eq!(commits[0].linked_issues[2].owner.as_deref(), Some("owner"));
    assert_eq!(commits[0].linked_issues[2].repo.as_deref(), Some("repo"));
    assert_eq!(
        commits[0].body.as_deref(),
        Some(
            r#"We'll have a play extempore. The play's the thing wherein I'll catch
the conscience of the king.

This mechanism allows for the revelation of truth through theatrical
performance, mirroring reality back to the audience."#
        )
    );

    assert_eq!(commits[0].trailers.len(), 2);
    match &commits[0].trailers[0] {
        GitTrailer::SignedOffBy { name, email } => {
            assert_eq!(name, "William Shakespeare");
            assert_eq!(email.as_deref(), Some("will@globe-theatre.com"));
        }
        _ => panic!("Expected SignedOffBy trailer"),
    }
    match &commits[0].trailers[1] {
        GitTrailer::CoAuthoredBy { name, email } => {
            assert_eq!(name, "Christopher Marlowe");
            assert_eq!(email.as_deref(), Some("kit@rose-theatre.com"));
        }
        _ => panic!("Expected CoAuthoredBy trailer"),
    }

    Ok(())
}

fn short_sha(oid: &Oid) -> String {
    oid.to_string()[..7].to_string()
}

#[test]
fn resolves_range_between_existing_releases() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        (tag: v3.0.0) To be, or not to be, that is the question
        (tag: v2.0.0) All the world's a stage
        (tag: v1.0.0) What's in a name? That which we call a rose
    ",
    )?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let range = git_repo.history(Some("v3.0.0".to_string()), None)?.range;

    assert_eq!(range.from, "v3.0.0");
    assert_eq!(range.to.as_deref(), Some("v2.0.0"));
    Ok(())
}

#[test]
fn resolves_range_from_tagged_head() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        (tag: v2.0.0) All the world's a stage
        (tag: v1.0.0) What's in a name? That which we call a rose
    ",
    )?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let range = git_repo.history(None, None)?.range;

    assert_eq!(range.from, "v2.0.0");
    assert_eq!(range.to.as_deref(), Some("v1.0.0"));
    Ok(())
}

#[test]
fn resolves_range_from_untagged_head_to_latest_release() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        The course of true love never did run smooth
        (tag: 1.0.0) Cowards die many times before their deaths
    ",
    )?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let range = git_repo.history(None, None)?.range;

    assert_eq!(range.from, short_sha(&test_repo.commits[1]));
    assert_eq!(range.to.as_deref(), Some("1.0.0"));
    Ok(())
}

#[test]
fn resolves_range_with_explicit_to() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        (tag: v3.0.0) To be, or not to be, that is the question
        (tag: v2.0.0) All the world's a stage
        Though this be madness, yet there is method in't
        (tag: v1.0.0) What's in a name? That which we call a rose
    ",
    )?;

    let git_repo = GitRepo::open(test_repo.path())?;

    let range = git_repo
        .history(Some("v3.0.0".to_string()), Some("v1.0.0".to_string()))?
        .range;
    assert_eq!(range.to.as_deref(), Some("v1.0.0"));

    let untagged = test_repo.commits[1].to_string();
    let range = git_repo
        .history(Some("v3.0.0".to_string()), Some(untagged))?
        .range;
    assert_eq!(range.to, Some(short_sha(&test_repo.commits[1])));
    Ok(())
}

#[test]
fn resolves_range_without_previous_release() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        The better part of valor is discretion
        Lord, what fools these mortals be!
    ",
    )?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let range = git_repo.history(None, None)?.range;

    assert_eq!(range.from, short_sha(&test_repo.commits[1]));
    assert_eq!(range.to, None);
    Ok(())
}

#[test]
fn resolves_untagged_ref_to_an_unambiguous_abbreviation() -> Result<()> {
    let test_repo =
        TestRepo::from_log("(tag: v1.0.0) What's in a name? That which we call a rose")?;
    let parent = test_repo.repo.find_commit(test_repo.commits[0])?;
    let signature = format!("{TEST_USER_NAME} <{TEST_USER_EMAIL}> {BASE_TIMESTAMP} +0000");

    let commit_object = |n: usize| {
        format!(
            "tree {}\nparent {}\nauthor {signature}\ncommitter {signature}\n\nThe rest is silence {n}\n",
            parent.tree_id(),
            parent.id(),
        )
    };

    // Search for two commits whose IDs share the same seven-character prefix
    let mut seen: HashMap<String, usize> = HashMap::new();
    let (first, second) = (0..)
        .find_map(|n| {
            let oid = Oid::hash_object(ObjectType::Commit, commit_object(n).as_bytes()).ok()?;
            seen.insert(oid.to_string()[..7].to_string(), n)
                .map(|m| (m, n))
        })
        .unwrap();

    let odb = test_repo.repo.odb()?;
    let target = odb.write(ObjectType::Commit, commit_object(first).as_bytes())?;
    odb.write(ObjectType::Commit, commit_object(second).as_bytes())?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let range = git_repo.history(Some(target.to_string()), None)?.range;

    assert_eq!(test_repo.repo.revparse_single(&range.from)?.id(), target);
    Ok(())
}

#[test]
fn preserves_lines_with_oversized_linked_issue_numbers() -> Result<()> {
    let mut test_repo = TestRepo::new()?;
    test_repo.commit(
        "fix: to thine own self be true
Closes #99999999999999999999
Fixes globe-theatre/hamlet#99999999999999999999
closes #7
The rest is silence.",
    )?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let commits = git_repo.history(None, None)?.commits;

    assert_eq!(
        commits[0].body.as_deref(),
        Some(
            "Closes #99999999999999999999\nFixes globe-theatre/hamlet#99999999999999999999\nThe rest is silence."
        )
    );
    assert_eq!(commits[0].linked_issues.len(), 1);
    assert_eq!(commits[0].linked_issues[0].number, 7);
    Ok(())
}

// main:        v1.0.0 ── v2.0.0 ── c2 ── v2.1.0
//                 └── v1.0.1 (backport, committed between v2.0.0 and v2.1.0)
fn repo_with_backport() -> Result<TestRepo> {
    let mut test_repo = TestRepo::new()?;
    let v1_0_0 = test_repo.commit("feat: what's in a name? That which we call a rose")?;
    let v2_0_0 = test_repo.commit("feat: all the world's a stage")?;
    test_repo.commit("feat: to be, or not to be, that is the question")?;
    let v2_1_0 = test_repo.commit("feat: brevity is the soul of wit")?;
    let v1_0_1 = test_repo.commit_on(
        &[v1_0_0],
        "fix: the lady doth protest too much, methinks",
        BASE_TIMESTAMP + 2,
    )?;

    test_repo.create_tag("v1.0.0", v1_0_0)?;
    test_repo.create_tag("v2.0.0", v2_0_0)?;
    test_repo.create_tag("v2.1.0", v2_1_0)?;
    test_repo.create_tag("v1.0.1", v1_0_1)?;
    Ok(test_repo)
}

#[test]
fn ignores_newer_backport_tag_when_selecting_previous_release() -> Result<()> {
    let test_repo = repo_with_backport()?;
    let git_repo = GitRepo::open(test_repo.path())?;

    let history = git_repo.history(Some("v2.1.0".to_string()), None)?;

    assert_eq!(history.range.to.as_deref(), Some("v2.0.0"));
    let first_lines: Vec<_> = history
        .commits
        .iter()
        .map(|c| c.first_line.as_str())
        .collect();
    assert_eq!(
        first_lines,
        vec![
            "feat: brevity is the soul of wit",
            "feat: to be, or not to be, that is the question"
        ]
    );
    Ok(())
}

#[test]
fn selects_branch_ancestor_as_previous_release_for_backport() -> Result<()> {
    let test_repo = repo_with_backport()?;
    let git_repo = GitRepo::open(test_repo.path())?;

    let history = git_repo.history(Some("v1.0.1".to_string()), None)?;

    assert_eq!(history.range.to.as_deref(), Some("v1.0.0"));
    let first_lines: Vec<_> = history
        .commits
        .iter()
        .map(|c| c.first_line.as_str())
        .collect();
    assert_eq!(
        first_lines,
        vec!["fix: the lady doth protest too much, methinks"]
    );
    Ok(())
}

#[test]
fn ignores_newer_backport_tag_from_untagged_head() -> Result<()> {
    let mut test_repo = TestRepo::new()?;
    let v1_0_0 = test_repo.commit("feat: what's in a name? That which we call a rose")?;
    test_repo.commit("feat: all the world's a stage")?;
    let v1_0_1 = test_repo.commit_on(
        &[v1_0_0],
        "fix: the lady doth protest too much, methinks",
        BASE_TIMESTAMP + 10,
    )?;
    test_repo.create_tag("v1.0.0", v1_0_0)?;
    test_repo.create_tag("v1.0.1", v1_0_1)?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let history = git_repo.history(None, None)?;

    assert_eq!(history.range.to.as_deref(), Some("v1.0.0"));
    let first_lines: Vec<_> = history
        .commits
        .iter()
        .map(|c| c.first_line.as_str())
        .collect();
    assert_eq!(first_lines, vec!["feat: all the world's a stage"]);
    Ok(())
}

#[test]
fn prefers_highest_semver_when_a_commit_has_several_tags() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        Now is the winter of our discontent
        (tag: v1.9.0, v2.0.0) All the world's a stage
        (tag: v1.0.0) What's in a name? That which we call a rose
    ",
    )?;
    let git_repo = GitRepo::open(test_repo.path())?;

    let tagged = git_repo.history(Some("v2.0.0".to_string()), None)?;
    assert_eq!(tagged.range.from, "v2.0.0");
    assert_eq!(tagged.range.to.as_deref(), Some("v1.0.0"));
    assert_eq!(tagged.commits.len(), 1);

    let head = git_repo.history(None, None)?;
    assert_eq!(head.range.to.as_deref(), Some("v2.0.0"));
    Ok(())
}

fn release_note(path: &Path, args: &[&str]) -> std::process::Output {
    // Stops CI detection resolving contributors through the platform API
    std::process::Command::new(env!("CARGO_BIN_EXE_release-note"))
        .args(args)
        .arg("--path")
        .arg(path)
        .env_remove("GITHUB_ACTIONS")
        .env_remove("GITLAB_CI")
        .output()
        .unwrap()
}

fn release_note_stdout(path: &Path, args: &[&str]) -> String {
    let output = release_note(path, args);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn release_heading(path: &Path) -> String {
    release_note_stdout(path, &[])
        .lines()
        .next()
        .unwrap()
        .to_string()
}

fn ranged_repo() -> Result<TestRepo> {
    TestRepo::from_log(
        "
        feat: now is the winter of our discontent
        (tag: v1.0.0) feat: all the world's a stage
        (tag: v0.9.0) fix: what's in a name?
        feat: to be or not to be
    ",
    )
}

#[test]
fn accepts_a_git_style_range() -> Result<()> {
    let test_repo = ranged_repo()?;

    let note = release_note_stdout(test_repo.path(), &["v0.9.0..v1.0.0"]);
    assert!(note.starts_with("## v1.0.0 - "), "{note}");
    assert!(note.contains("all the world's a stage"), "{note}");
    assert!(!note.contains("now is the winter"), "{note}");
    assert!(!note.contains("what's in a name?"), "{note}");
    Ok(())
}

#[test]
fn range_without_an_end_reaches_head() -> Result<()> {
    let test_repo = ranged_repo()?;

    let note = release_note_stdout(test_repo.path(), &["v0.9.0.."]);
    assert!(note.contains("now is the winter"), "{note}");
    assert!(note.contains("all the world's a stage"), "{note}");
    assert!(!note.contains("what's in a name?"), "{note}");
    Ok(())
}

#[test]
fn single_ref_measures_from_the_previous_release() -> Result<()> {
    let test_repo = ranged_repo()?;

    let note = release_note_stdout(test_repo.path(), &["v1.0.0"]);
    assert!(note.starts_with("## v1.0.0 - "), "{note}");
    assert!(note.contains("all the world's a stage"), "{note}");
    assert!(!note.contains("what's in a name?"), "{note}");
    Ok(())
}

#[test]
fn rejects_a_symmetric_difference_range() -> Result<()> {
    let test_repo = ranged_repo()?;

    let output = release_note(test_repo.path(), &["v0.9.0...v1.0.0"]);
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("v0.9.0..v1.0.0"), "{stderr}");
    Ok(())
}

#[test]
fn rejects_two_positional_refs() -> Result<()> {
    let test_repo = ranged_repo()?;

    let output = release_note(test_repo.path(), &["v1.0.0", "v0.9.0"]);
    assert!(!output.status.success(), "{output:?}");
    Ok(())
}

#[test]
fn names_release_after_semver_tag_at_head_over_other_tags() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        (tag: latest, v1.2.3) feat: all the world's a stage
        (tag: v1.0.0) fix: what's in a name?
    ",
    )?;

    let heading = release_heading(test_repo.path());
    assert!(heading.starts_with("## v1.2.3 - "), "{heading}");
    Ok(())
}

#[test]
fn names_release_after_highest_semver_tag_at_head() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        (tag: v1.2.3, v1.2.4) feat: all the world's a stage
        (tag: v1.0.0) fix: what's in a name?
    ",
    )?;

    let heading = release_heading(test_repo.path());
    assert!(heading.starts_with("## v1.2.4 - "), "{heading}");
    Ok(())
}

// main:        v1.0.0 ── v2.0.0 ── merge ── v2.1.0
//                 └── v1.0.1 ──────┘ (backport, tagged after v2.0.0, merged into main)
#[test]
fn ignores_merged_backport_tag_when_selecting_previous_release() -> Result<()> {
    let mut test_repo = TestRepo::new()?;
    let v1_0_0 = test_repo.commit("feat: what's in a name? That which we call a rose")?;
    let v2_0_0 = test_repo.commit("feat: all the world's a stage")?;
    let v1_0_1 = test_repo.commit_on(
        &[v1_0_0],
        "fix: the lady doth protest too much, methinks",
        BASE_TIMESTAMP + 10,
    )?;
    test_repo.merge(v1_0_1, "chore: merge the v1 maintenance branch")?;
    let v2_1_0 = test_repo.commit("feat: brevity is the soul of wit")?;

    test_repo.create_tag("v1.0.0", v1_0_0)?;
    test_repo.create_tag("v2.0.0", v2_0_0)?;
    test_repo.create_tag("v1.0.1", v1_0_1)?;
    test_repo.create_tag("v2.1.0", v2_1_0)?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let history = git_repo.history(Some("v2.1.0".to_string()), None)?;

    assert_eq!(history.range.to.as_deref(), Some("v2.0.0"));
    let first_lines: Vec<_> = history
        .commits
        .iter()
        .map(|c| c.first_line.as_str())
        .collect();
    assert!(
        !first_lines.contains(&"feat: all the world's a stage"),
        "v2.0.0's commit was already released: {first_lines:?}"
    );
    Ok(())
}

#[test]
fn labels_explicit_refs_with_the_requested_tag() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        Now is the winter of our discontent
        (tag: v1.9.0, v2.0.0) All the world's a stage
        (tag: v1.0.0) What's in a name? That which we call a rose
    ",
    )?;
    let git_repo = GitRepo::open(test_repo.path())?;

    let from = git_repo.history(Some("v1.9.0".to_string()), None)?;
    assert_eq!(from.range.from, "v1.9.0");

    let to = git_repo.history(None, Some("v1.9.0".to_string()))?;
    assert_eq!(to.range.to.as_deref(), Some("v1.9.0"));
    Ok(())
}

// A merge whose parents reach both v2.0.0 and v1.0.0, which descends from it.
// v2.0.0 carries the newest commit date and the higher version, so date order
// alone would wrongly treat it as a candidate for the previous release.
//
//   v2.0.0 (skewed: newest date) ── v1.0.0 ──┐
//         └───────────────────────────────── merge ── v3.0.0
#[test]
fn only_nearest_tags_are_candidates_when_commit_dates_are_skewed() -> Result<()> {
    let mut test_repo = TestRepo::new()?;
    let root = test_repo.commit("feat: what's in a name? That which we call a rose")?;
    let ancestor = test_repo.commit_on(
        &[root],
        "feat: all the world's a stage",
        BASE_TIMESTAMP + 100,
    )?;
    let nearest = test_repo.commit_on(
        &[ancestor],
        "fix: the lady doth protest too much, methinks",
        BASE_TIMESTAMP + 1,
    )?;
    let merge = test_repo.commit_on(
        &[nearest, ancestor],
        "chore: merge the hotfix",
        BASE_TIMESTAMP + 2,
    )?;
    let from = test_repo.commit_on(
        &[merge],
        "feat: brevity is the soul of wit",
        BASE_TIMESTAMP + 3,
    )?;

    test_repo.create_tag("v2.0.0", ancestor)?;
    test_repo.create_tag("v1.0.0", nearest)?;
    test_repo.create_tag("v3.0.0", from)?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let history = git_repo.history(Some("v3.0.0".to_string()), None)?;

    assert_eq!(history.range.to.as_deref(), Some("v1.0.0"));
    Ok(())
}

// main:        v1.0.0 ── v2.0.0
//                 │         └──────────┐
// maintenance:    └── v1.0.1 ──── merge ── v1.0.2
#[test]
fn previous_release_is_lower_than_a_tagged_from() -> Result<()> {
    let mut test_repo = TestRepo::new()?;
    let v1_0_0 = test_repo.commit("feat: what's in a name? That which we call a rose")?;
    let v2_0_0 = test_repo.commit("feat: all the world's a stage")?;
    let v1_0_1 = test_repo.commit_on(
        &[v1_0_0],
        "fix: the lady doth protest too much, methinks",
        BASE_TIMESTAMP + 10,
    )?;
    let merge = test_repo.commit_on(
        &[v1_0_1, v2_0_0],
        "chore: merge main into maintenance",
        BASE_TIMESTAMP + 11,
    )?;
    let v1_0_2 = test_repo.commit_on(
        &[merge],
        "fix: brevity is the soul of wit",
        BASE_TIMESTAMP + 12,
    )?;

    test_repo.create_tag("v1.0.0", v1_0_0)?;
    test_repo.create_tag("v2.0.0", v2_0_0)?;
    test_repo.create_tag("v1.0.1", v1_0_1)?;
    test_repo.create_tag("v1.0.2", v1_0_2)?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let history = git_repo.history(Some("v1.0.2".to_string()), None)?;

    assert_eq!(history.range.to.as_deref(), Some("v1.0.1"));
    Ok(())
}

#[test]
fn ceiling_comes_from_the_requested_tag() -> Result<()> {
    let test_repo = TestRepo::from_log(
        "
        (tag: v1.9.0, v2.0.0) All the world's a stage
        (tag: v1.10.0) Brevity is the soul of wit
        (tag: v1.0.0) What's in a name? That which we call a rose
    ",
    )?;
    let git_repo = GitRepo::open(test_repo.path())?;

    let history = git_repo.history(Some("v1.9.0".to_string()), None)?;

    assert_eq!(history.range.from, "v1.9.0");
    assert_eq!(history.range.to.as_deref(), Some("v1.0.0"));
    Ok(())
}

// main:        v1.0.0 ── v2.0.0
//                 │         └─────┐
// maintenance:    └── fix ──── merge ── v1.0.1
#[test]
fn walks_past_newer_tags_to_a_lower_previous_release() -> Result<()> {
    let mut test_repo = TestRepo::new()?;
    let v1_0_0 = test_repo.commit("feat: what's in a name? That which we call a rose")?;
    let v2_0_0 = test_repo.commit("feat: all the world's a stage")?;
    let fix = test_repo.commit_on(
        &[v1_0_0],
        "fix: the lady doth protest too much, methinks",
        BASE_TIMESTAMP + 10,
    )?;
    let merge = test_repo.commit_on(
        &[fix, v2_0_0],
        "chore: merge main into maintenance",
        BASE_TIMESTAMP + 11,
    )?;
    let v1_0_1 = test_repo.commit_on(
        &[merge],
        "fix: brevity is the soul of wit",
        BASE_TIMESTAMP + 12,
    )?;

    test_repo.create_tag("v1.0.0", v1_0_0)?;
    test_repo.create_tag("v2.0.0", v2_0_0)?;
    test_repo.create_tag("v1.0.1", v1_0_1)?;

    let git_repo = GitRepo::open(test_repo.path())?;
    let history = git_repo.history(Some("v1.0.1".to_string()), None)?;

    assert_eq!(history.range.to.as_deref(), Some("v1.0.0"));
    Ok(())
}
