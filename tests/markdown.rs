mod commit;

use commit::CommitBuilder;
use release_note::analyzer::{CategorizedCommits, CommitCategory, ContributorSummary};
use release_note::markdown;
use release_note::platform::Platform;
use std::collections::HashMap;

// Fixed timestamp for tests: November 27, 2025 00:00:00 UTC
const TEST_RELEASE_DATE: i64 = 1764201600;

#[test]
fn generates_release_note_from_multiple_categories() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Breaking,
        vec![
            CommitBuilder::new("feat!: the course of true love never did run smooth")
                .with_body("Lord, what fools these mortals be! The lunatic, the lover and the poet are of imagination all compact.")
                .build(),
            CommitBuilder::new("refactor(york)!: now is the winter of our discontent")
                .with_body("BREAKING CHANGE: made glorious summer by this sun of York.")
                .build(),
        ],
    );

    by_category.insert(
        CommitCategory::Feature,
        vec![
            CommitBuilder::new("feat: all the world's a stage")
                .with_body("And all the men and women merely players. They have their exits and their entrances; and one man in his time plays many parts.")
                .build(),
            CommitBuilder::new("feat: to be or not to be")
                .build(),
        ],
    );

    by_category.insert(
        CommitCategory::Fix,
        vec![CommitBuilder::new("fix: though she be but little, she is fierce")
            .with_body("Some are born great, some achieve greatness, and some have greatness thrust upon them.")
            .build()],
    );

    by_category.insert(
        CommitCategory::Dependencies,
        vec![
            CommitBuilder::new("chore(deps): all that glisters is not gold").build(),
            CommitBuilder::new("fix(deps): the better part of valor is discretion").build(),
        ],
    );

    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn displays_contributors_with_github_commit_links() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![
            CommitBuilder::new("feat: the course of true love never did run smooth")
                .with_contributor("shakespeare")
                .with_timestamp(1748390400)
                .build(),
            CommitBuilder::new("feat: some Cupid kills with arrows, some with traps")
                .with_contributor("shakespeare")
                .with_timestamp(1748476800)
                .build(),
            CommitBuilder::new("feat: all the world's a stage")
                .with_contributor("marlowe")
                .with_timestamp(1748390400)
                .build(),
        ],
    );

    let contributors = vec![
        ContributorSummary {
            username: "shakespeare".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/2651292?v=4".to_string(),
            count: 2,
            is_bot: false,
            first_commit_timestamp: 1748390400,
            last_commit_timestamp: 1748476800,
        },
        ContributorSummary {
            username: "marlowe".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/2651292?v=4".to_string(),
            count: 1,
            is_bot: false,
            first_commit_timestamp: 1748390400,
            last_commit_timestamp: 1748390400,
        },
    ];

    let platform = Platform::GitHub {
        url: "https://github.com/shakespeare/globe-theatre".to_string(),
        api_url: "https://api.github.com".to_string(),
        owner: "shakespeare".to_string(),
        repo: "globe-theatre".to_string(),
    };

    let categorized = CategorizedCommits {
        by_category,
        contributors,
    };
    let result =
        markdown::render_history(&categorized, &platform, "v1.0.0", TEST_RELEASE_DATE).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn displays_contributors_without_links_for_gitlab() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: all the world's a stage").build()],
    );

    let contributors = vec![
        ContributorSummary {
            username: "hamlet".to_string(),
            avatar_url: "https://gitlab.com/uploads/-/system/user/avatar/123/avatar.png"
                .to_string(),
            count: 3,
            is_bot: false,
            first_commit_timestamp: 1748390400,
            last_commit_timestamp: 1748476800,
        },
        ContributorSummary {
            username: "ophelia".to_string(),
            avatar_url: "https://gitlab.com/uploads/-/system/user/avatar/456/avatar.png"
                .to_string(),
            count: 1,
            is_bot: false,
            first_commit_timestamp: 1748390400,
            last_commit_timestamp: 1748390400,
        },
    ];

    let platform = Platform::GitLab {
        url: "https://gitlab.com/shakespeare/globe-theatre".to_string(),
        api_url: "https://gitlab.com/api/v4".to_string(),
        graphql_url: "https://gitlab.com/api/graphql".to_string(),
        project_path: "shakespeare/globe-theatre".to_string(),
    };

    let categorized = CategorizedCommits {
        by_category,
        contributors,
    };
    let result =
        markdown::render_history(&categorized, &platform, "v1.0.0", TEST_RELEASE_DATE).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn unwraps_paragraphs_to_single_lines() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![
            CommitBuilder::new("feat: add the quality of mercy soliloquy")
                .with_body(
                    "The quality of mercy is not strained.
It droppeth as the gentle rain from heaven
upon the place beneath. It is twice blessed:
it blesseth him that gives and him that takes.

'Tis mightiest in the mightiest; it becomes
the throned monarch better than his crown.
His scepter shows the force of temporal power,
the attribute to awe and majesty.",
                )
                .build(),
        ],
    );

    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn unwraps_list_items_to_single_lines() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: the seven ages of man")
            .with_body(
                "All the world's a stage, and all the men and women merely players. They have their exits and their entrances; and one man in his time plays many parts, his acts being seven ages:

- The infant, mewling and puking in the nurse's arms, knowing naught of the world that awaits
- The whining school-boy with his satchel and shining morning face, creeping like snail unwillingly to school
- The lover, sighing like furnace, with a woeful ballad made to his mistress' eyebrow
- The soldier, full of strange oaths and bearded like the pard, jealous in honour, sudden and quick in quarrel
- The justice, in fair round belly with good capon lined, with eyes severe and beard of formal cut

That is the last scene of all, that ends this strange eventful history.",
            )
            .build()],
    );

    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn unwraps_numbered_lists_to_single_lines() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: instructions for wooing fair maidens")
            .with_body(
                "When wooing a lady of quality, attend to these principles with utmost care and devotion:

1. First, compose sonnets praising her beauty in terms most eloquent, comparing her eyes to stars and her voice to sweetest music
2. Second, present tokens of affection such as posies of flowers gathered from the fairest gardens in the realm
3. Third, demonstrate thy valour and honour through noble deeds that shall be sung by minstrels across the land",
            )
            .build()],
    );

    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn preserves_code_blocks_without_wrapping() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: add theatrical script formatting")
            .with_body(
                "The script format preserves the playwright's original line formatting without alteration:

```
HAMLET: To be, or not to be, that is the question—whether 'tis nobler in the mind to suffer the slings and arrows of outrageous fortune, or to take arms against a sea of troubles
OPHELIA: Good my lord, how does your honour for this many a day?
```

These lines must maintain their integrity as written by the immortal bard.",
            )
            .build()],
    );

    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn preserves_block_quotes_as_is() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: add wisdom from the bard")
            .with_body(
                "From the great playwright's most celebrated work on the nature of existence:

> To be, or not to be, that is the question—whether 'tis nobler in the mind to suffer the slings and arrows of outrageous fortune, or to take arms against a sea of troubles and by opposing end them.

This soliloquy explores the fundamental nature of human existence and mortality.",
            )
            .build()],
    );

    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn handles_mixed_structured_content() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: comprehensive guide to staging Hamlet")
            .with_body(
                "This production combines the traditional elements of Elizabethan theatre with modern interpretations:

Principal considerations:
- The soliloquies must be delivered with proper contemplation and pause for dramatic effect
- Staging of the ghost requires atmospheric lighting and ethereal movement across the stage
- Sword choreography in the final duel demands precision and theatrical flourish

Performance notes:

1. Hamlet's madness should transition from feigned to genuine throughout the five acts
2. Ophelia's descent into madness must contrast with Hamlet's calculated performance
3. The play-within-a-play scene requires careful direction to maintain audience attention

Stage directions example:

```
[Ghost beckons HAMLET to follow. Exeunt GHOST and HAMLET]
HORATIO: He waxes desperate with imagination.
```

> Note: The text of the First Folio differs significantly from the Second Quarto and should be consulted for alternate readings.

Additional context on Elizabethan staging conventions is essential for authentic production.",
            )
            .build()],
    );

    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn generates_no_release_note_when_no_commits() {
    let by_category = HashMap::new();
    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn excludes_git_trailers() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![
            CommitBuilder::new("feat: the lady doth protest too much, methinks")
                .with_body("Though this be madness, yet there is method in't.")
                .with_trailer("Signed-off-by", "William Shakespeare <will@globe-theatre.com>")
                .with_trailer("Co-authored-by", "Christopher Marlowe <kit@rose-theatre.com>")
                .build(),
            CommitBuilder::new("feat: brevity is the soul of wit")
                .with_body("To be, or not to be, that is the question:\n- Whether 'tis nobler in the mind to suffer\n- The slings and arrows of outrageous fortune\n\nOr to take arms against a sea of troubles.")
                .with_trailer("Reviewed-by", "Ben Jonson <ben@theatre.com>")
                .build(),
        ],
    );

    by_category.insert(
        CommitCategory::Fix,
        vec![
            CommitBuilder::new("fix: something is rotten in the state of Denmark")
                .with_body("The rest is silence.")
                .with_trailer("Acked-by", "Hamlet <hamlet@elsinore.dk>")
                .build(),
        ],
    );

    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn displays_multiple_contributors() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![
            CommitBuilder::new("feat: we are such stuff as dreams are made on")
                .with_contributors(vec!["shakespeare", "marlowe", "jonson"])
                .build(),
            CommitBuilder::new("feat: some Cupid kills with arrows, some with traps")
                .with_contributor("shakespeare")
                .build(),
        ],
    );

    let contributors = vec![
        ContributorSummary {
            username: "shakespeare".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/2651292?v=4".to_string(),
            count: 2,
            is_bot: false,
            first_commit_timestamp: 1564567890,
            last_commit_timestamp: 1564567891,
        },
        ContributorSummary {
            username: "jonson".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/2651292?v=4".to_string(),
            count: 1,
            is_bot: false,
            first_commit_timestamp: 1564567890,
            last_commit_timestamp: 1564567890,
        },
        ContributorSummary {
            username: "marlowe".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/2651292?v=4".to_string(),
            count: 1,
            is_bot: false,
            first_commit_timestamp: 1564567890,
            last_commit_timestamp: 1564567890,
        },
    ];

    let categorized = CategorizedCommits {
        by_category,
        contributors,
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn filters_bot_contributors() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![
            CommitBuilder::new("feat: the course of true love never did run smooth")
                .with_contributor("shakespeare")
                .build(),
            CommitBuilder::new("feat: a plague o' both your houses")
                .with_contributor_bot("iago[bot]")
                .build(),
        ],
    );

    let contributors = vec![
        ContributorSummary {
            username: "shakespeare".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/2651292?v=4".to_string(),
            count: 1,
            is_bot: false,
            first_commit_timestamp: 1564567890,
            last_commit_timestamp: 1564567890,
        },
        ContributorSummary {
            username: "iago[bot]".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/2651292?v=4".to_string(),
            count: 1,
            is_bot: true,
            first_commit_timestamp: 1564567890,
            last_commit_timestamp: 1564567890,
        },
    ];

    let categorized = CategorizedCommits {
        by_category,
        contributors,
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn preserves_tables_without_unwrapping() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![
            CommitBuilder::new("feat: add comparison of Shakespeare's great tragedies")
                .with_body(
                    "This feature adds a comprehensive overview of the four great tragedies, \
allowing users to compare key elements across these masterworks of \
Elizabethan drama.

| Play     | Year | Protagonist | Fatal Flaw       |
|----------|------|-------------|------------------|
| Hamlet   | 1601 | Hamlet      | Indecision       |
| Othello  | 1604 | Othello     | Jealousy         |
| King Lear| 1606 | Lear        | Vanity           |
| Macbeth  | 1606 | Macbeth     | Ambition         |

Each tragedy explores the downfall of a noble figure through their own \
weaknesses, reflecting the Aristotelian concept of hamartia that \
Shakespeare so masterfully employed.",
                )
                .build(),
        ],
    );

    let categorized = CategorizedCommits {
        by_category,
        contributors: Vec::new(),
    };
    let result =
        markdown::render_history(&categorized, &Platform::Unknown, "HEAD", TEST_RELEASE_DATE)
            .unwrap();

    insta::assert_snapshot!(result);
}
