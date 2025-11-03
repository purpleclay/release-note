mod commit;

use commit::CommitBuilder;
use release_note::analyzer::{CommitAnalyzer, CommitCategory};

#[test]
fn categorizes_commits() {
    let test_cases = vec![
        ("feat: to be or not to be", CommitCategory::Feature),
        ("fix: all the world's a stage", CommitCategory::Fix),
        (
            "docs: a horse! a horse! my kingdom for a horse!",
            CommitCategory::Documentation,
        ),
        (
            "build: if music be the food of love, play on",
            CommitCategory::Other,
        ),
        (
            "style: lord, what fools these mortals be!",
            CommitCategory::Other,
        ),
        (
            "refactor: cowards die many times before their deaths",
            CommitCategory::Refactor,
        ),
        (
            "perf: something is rotten in the state of denmark",
            CommitCategory::Performance,
        ),
        (
            "test: the lady doth protest too much, methinks",
            CommitCategory::Test,
        ),
        (
            "ci: though this be madness, yet there is method in't",
            CommitCategory::CI,
        ),
        (
            "chore: now is the winter of our discontent",
            CommitCategory::Chore,
        ),
        (
            "there is a tide in the affairs of men",
            CommitCategory::Other,
        ),
    ];

    for (commit_msg, expected_category) in test_cases {
        let commit = CommitBuilder::new(commit_msg).build();
        let result = CommitAnalyzer::analyze(&[commit]);
        let commit = result.by_category.get(&expected_category).unwrap();
        assert_eq!(commit.len(), 1);
        assert_eq!(commit[0].first_line, commit_msg);
    }
}

#[test]
fn categorizes_by_breaking_change_in_footer() {
    let commit = CommitBuilder::new("fix: the course of true love never did run smooth")
        .with_footer("BREAKING CHANGE: but in battalions")
        .build();
    let result = CommitAnalyzer::analyze(&[commit]);
    let breaking = result.by_category.get(&CommitCategory::Breaking).unwrap();
    assert_eq!(breaking.len(), 1);
    assert_eq!(
        breaking[0].first_line,
        "fix: the course of true love never did run smooth"
    );
}

#[test]
fn categorizes_breaking_change_by_hash_bang() {
    let commit =
        CommitBuilder::new("refactor!: when sorrows come, they come not single spies").build();
    let result = CommitAnalyzer::analyze(&[commit]);
    let breaking = result.by_category.get(&CommitCategory::Breaking).unwrap();
    assert_eq!(breaking.len(), 1);
    assert_eq!(
        breaking[0].first_line,
        "refactor!: when sorrows come, they come not single spies"
    );
}

#[test]
fn categorizes_commits_while_retaining_order() {
    let commits = vec![
        CommitBuilder::new("feat: love all, trust a few, do wrong to none").build(),
        CommitBuilder::new("fix: some rise by sin, and some by virtue fall").build(),
        CommitBuilder::new("feat: be not afraid of greatness").build(),
        CommitBuilder::new("feat: hell is empty and all the devils are here").build(),
        CommitBuilder::new("fix: brevity is the soul of wit").build(),
    ];

    let result = CommitAnalyzer::analyze(&commits);

    let features = result.by_category.get(&CommitCategory::Feature).unwrap();
    assert_eq!(features.len(), 3);
    assert_eq!(
        features[0].first_line,
        "feat: love all, trust a few, do wrong to none"
    );
    assert_eq!(features[1].first_line, "feat: be not afraid of greatness");
    assert_eq!(
        features[2].first_line,
        "feat: hell is empty and all the devils are here"
    );

    let fixes = result.by_category.get(&CommitCategory::Fix).unwrap();
    assert_eq!(fixes.len(), 2);
    assert_eq!(
        fixes[0].first_line,
        "fix: some rise by sin, and some by virtue fall"
    );
    assert_eq!(fixes[1].first_line, "fix: brevity is the soul of wit");
}

#[test]
fn categorizes_by_dependency_scope() {
    let commits = vec![
        CommitBuilder::new("feat(deps): all that glisters is not gold").build(),
        CommitBuilder::new("fix(deps): give every man thy ear, but few thy voice").build(),
        CommitBuilder::new("chore(deps): the better part of valor is discretion").build(),
        CommitBuilder::new("test(deps): we are such stuff as dreams are made on").build(),
        CommitBuilder::new("perf(deps): the fault, dear Brutus, is not in our stars").build(),
    ];

    let result = CommitAnalyzer::analyze(&commits);

    let deps = result
        .by_category
        .get(&CommitCategory::Dependencies)
        .unwrap();
    assert_eq!(deps.len(), 5);
}
