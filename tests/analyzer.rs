mod commit;

use commit::CommitBuilder;
use release_note::analyzer::CommitAnalyzer;

#[test]
fn parses_commit_types() {
    let test_cases = vec![
        ("feat: to be or not to be", "feat"),
        ("fix: all the world's a stage", "fix"),
        ("docs: a horse! a horse! my kingdom for a horse!", "docs"),
        ("build: if music be the food of love, play on", "build"),
        ("style: lord, what fools these mortals be!", "style"),
        (
            "refactor: cowards die many times before their deaths",
            "refactor",
        ),
        ("perf: something is rotten in the state of denmark", "perf"),
        ("test: the lady doth protest too much, methinks", "test"),
        ("ci: though this be madness, yet there is method in't", "ci"),
        ("chore: now is the winter of our discontent", "chore"),
        ("there is a tide in the affairs of men", ""),
    ];

    for (commit_msg, expected_type) in test_cases {
        let commit = CommitBuilder::new(commit_msg).build();
        let result = CommitAnalyzer::analyze(&[commit]);
        assert_eq!(result.commits.len(), 1);
        assert_eq!(result.commits[0].first_line, commit_msg);
        assert_eq!(result.commits[0].type_, expected_type);
    }
}

#[test]
fn detects_breaking_change_in_footer() {
    let commit = CommitBuilder::new("fix: the course of true love never did run smooth")
        .with_body(
            "When sorrows come, they come not single spies, but in battalions. \
First, her father slain; next, your son gone; and he most violent author \
of his own just remove.

The people muddied, thick and unwholesome in their thoughts and whispers \
for good Polonius' death, and we have done but greenly in hugger-mugger \
to inter him.

BREAKING CHANGE: but in battalions",
        )
        .build();
    let result = CommitAnalyzer::analyze(&[commit]);
    assert!(result.commits[0].breaking);
    assert_eq!(result.commits[0].type_, "fix");
}

#[test]
fn detects_breaking_change_by_hash_bang() {
    let commit =
        CommitBuilder::new("refactor(ui)!: when sorrows come, they come not single spies").build();
    let result = CommitAnalyzer::analyze(&[commit]);
    assert!(result.commits[0].breaking);
    assert_eq!(result.commits[0].type_, "refactor");
    assert_eq!(result.commits[0].scope, "ui");
}

#[test]
fn parses_dependency_scope() {
    let commits = vec![
        CommitBuilder::new("feat(deps): all that glisters is not gold").build(),
        CommitBuilder::new("fix(deps): give every man thy ear, but few thy voice").build(),
        CommitBuilder::new("chore(deps): the better part of valor is discretion").build(),
        CommitBuilder::new("test(deps): we are such stuff as dreams are made on").build(),
        CommitBuilder::new("perf(deps): the fault, dear Brutus, is not in our stars").build(),
    ];

    let result = CommitAnalyzer::analyze(&commits);

    assert!(result.commits.iter().all(|c| c.scope == "deps"));
}

#[test]
fn supports_mixed_case_commit_types() {
    let commits = vec![
        CommitBuilder::new("FEAT: a rose by any other name would smell as sweet").build(),
        CommitBuilder::new("Fix: the world's mine oyster").build(),
        CommitBuilder::new("Docs: we know what we are, but know not what we may be").build(),
        CommitBuilder::new("ChOrE: this above all: to thine own self be true").build(),
    ];

    let result = CommitAnalyzer::analyze(&commits);

    let types: Vec<_> = result.commits.iter().map(|c| c.type_.as_str()).collect();
    assert_eq!(types, vec!["feat", "fix", "docs", "chore"]);
}

#[test]
fn supports_flexible_spacing_in_commit_format() {
    let commits = vec![
        CommitBuilder::new("feat:the readiness is all").build(),
        CommitBuilder::new("fix:  strong reasons make strong actions").build(),
        CommitBuilder::new("feat(scope):delays have dangerous ends").build(),
        CommitBuilder::new("fix(scope) :  a man can die but once").build(),
    ];

    let result = CommitAnalyzer::analyze(&commits);

    let types: Vec<_> = result.commits.iter().map(|c| c.type_.as_str()).collect();
    assert_eq!(types, vec!["feat", "fix", "feat", "fix"]);
}

#[test]
fn supports_flexible_breaking_footer_formats() {
    let commits = vec![
        CommitBuilder::new("fix: frailty, thy name is woman")
            .with_body("BREAKING CHANGE: with mirth and laughter let old wrinkles come")
            .build(),
        CommitBuilder::new("feat: expectation is the root of all heartache")
            .with_body("BREAKING-CHANGE: misery acquaints a man with strange bedfellows")
            .build(),
        CommitBuilder::new("chore: uneasy lies the head that wears a crown")
            .with_body("breaking change: what's done is done")
            .build(),
        CommitBuilder::new("docs: some are born great")
            .with_body("Breaking-Changes: some achieve greatness")
            .build(),
        CommitBuilder::new("test: out, out, brief candle")
            .with_body("BREAKING CHANGES: life's but a walking shadow")
            .build(),
    ];

    let result = CommitAnalyzer::analyze(&commits);

    assert!(result.commits.iter().all(|c| c.breaking));
}

#[test]
fn detects_breaking_change_when_parsed_as_trailer() {
    let commit = CommitBuilder::new("refactor: parting is such sweet sorrow")
        .with_trailer("BREAKING CHANGE", "shall I compare thee to a summer's day")
        .with_trailer(
            "Co-authored-by",
            "Christopher Marlowe <kit@rose-theatre.com>",
        )
        .build();

    let result = CommitAnalyzer::analyze(&[commit]);
    assert!(result.commits[0].breaking);
}

#[test]
fn sets_breaking_true_for_bang_commits() {
    let commit = CommitBuilder::new("feat!: something breaking").build();
    let result = CommitAnalyzer::analyze(&[commit]);

    assert!(result.commits[0].breaking);
    assert_eq!(result.commits[0].breaking_description, None);
}

#[test]
fn sets_breaking_true_and_description_for_footer_commits() {
    let commit = CommitBuilder::new("fix: the course of true love never did run smooth")
        .with_body("BREAKING CHANGE: with mirth and laughter let old wrinkles come")
        .build();
    let result = CommitAnalyzer::analyze(&[commit]);

    assert!(result.commits[0].breaking);
    assert_eq!(
        result.commits[0].breaking_description,
        Some("with mirth and laughter let old wrinkles come".to_string())
    );
}

#[test]
fn sets_breaking_true_and_description_from_trailer() {
    let commit = CommitBuilder::new("refactor: parting is such sweet sorrow")
        .with_trailer("BREAKING-CHANGE", "shall I compare thee to a summer's day")
        .build();
    let result = CommitAnalyzer::analyze(&[commit]);

    assert!(result.commits[0].breaking);
    assert_eq!(
        result.commits[0].breaking_description,
        Some("shall I compare thee to a summer's day".to_string())
    );
}

#[test]
fn captures_multiline_breaking_description_from_body() {
    let commit = CommitBuilder::new("fix: the course of true love never did run smooth")
        .with_body(
            "BREAKING CHANGE: with mirth and laughter let old wrinkles come\nand so the whirligig of time brings in his revenges",
        )
        .build();
    let result = CommitAnalyzer::analyze(&[commit]);

    assert_eq!(
        result.commits[0].breaking_description,
        Some("with mirth and laughter let old wrinkles come\nand so the whirligig of time brings in his revenges".to_string())
    );
}

#[test]
fn non_breaking_commits_have_breaking_false() {
    let commits = vec![
        CommitBuilder::new("feat: a normal feature").build(),
        CommitBuilder::new("not conventional").build(),
    ];
    let result = CommitAnalyzer::analyze(&commits);

    for commit in &result.commits {
        assert!(!commit.breaking);
        assert_eq!(commit.breaking_description, None);
    }
}

#[test]
fn populates_scope_from_conventional_commit() {
    let commits = vec![
        CommitBuilder::new("feat(api): something scoped").build(),
        CommitBuilder::new("feat: something unscoped").build(),
        CommitBuilder::new("not a conventional commit").build(),
    ];

    let result = CommitAnalyzer::analyze(&commits);

    let scopes: Vec<_> = result.commits.iter().map(|c| c.scope.as_str()).collect();
    assert_eq!(scopes, vec!["api", "", ""]);
}

#[test]
fn populates_description_from_conventional_commit() {
    let commits = vec![
        CommitBuilder::new("feat(api): the quality of mercy is not strained").build(),
        CommitBuilder::new("feat!: once more unto the breach").build(),
        CommitBuilder::new("fix(scope) :  a man can die but once").build(),
        CommitBuilder::new("not a conventional commit").build(),
    ];

    let result = CommitAnalyzer::analyze(&commits);

    let descriptions: Vec<_> = result
        .commits
        .iter()
        .map(|c| c.description.as_str())
        .collect();
    assert_eq!(
        descriptions,
        vec![
            "the quality of mercy is not strained",
            "once more unto the breach",
            "a man can die but once",
            "not a conventional commit",
        ]
    );
}

#[test]
fn exposes_all_commits_in_original_order() {
    let commits = vec![
        CommitBuilder::new("feat: love all, trust a few, do wrong to none").build(),
        CommitBuilder::new("chore(deps): all that glisters is not gold").build(),
        CommitBuilder::new("fix!: some rise by sin, and some by virtue fall").build(),
        CommitBuilder::new("build(api): if music be the food of love, play on").build(),
        CommitBuilder::new("there is a tide in the affairs of men").build(),
    ];

    let result = CommitAnalyzer::analyze(&commits);

    let actual: Vec<_> = result
        .commits
        .iter()
        .map(|c| {
            (
                c.type_.as_str(),
                c.scope.as_str(),
                c.breaking,
                c.description.as_str(),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            ("feat", "", false, "love all, trust a few, do wrong to none"),
            ("chore", "deps", false, "all that glisters is not gold"),
            ("fix", "", true, "some rise by sin, and some by virtue fall"),
            (
                "build",
                "api",
                false,
                "if music be the food of love, play on"
            ),
            ("", "", false, "there is a tide in the affairs of men"),
        ]
    );
}

#[test]
fn detects_breaking_change_trailer_with_hyphen() {
    let commit = CommitBuilder::new("chore: all's well that ends well")
        .with_trailer("BREAKING-CHANGES", "the evil that men do lives after them")
        .with_trailer("Signed-off-by", "Ben Jonson <ben@theatre.com>")
        .build();

    let result = CommitAnalyzer::analyze(&[commit]);
    assert!(result.commits[0].breaking);
}
