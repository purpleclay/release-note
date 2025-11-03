mod commit;

use commit::CommitBuilder;
use release_note::analyzer::{CategorizedCommits, CommitCategory};
use release_note::markdown;
use std::collections::HashMap;

#[test]
fn generates_release_note_from_multiple_categories() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Breaking,
        vec![CommitBuilder::new("feat!: the course of true love never did run smooth")
            .with_body("Lord, what fools these mortals be! The lunatic, the lover and the poet are of imagination all compact.")
            .build()],
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

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn correctly_wraps_long_commit_body() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: now is the winter of our discontent")
            .with_body(
                "Made glorious summer by this sun of York; and all the clouds that lour'd upon our house in the deep bosom of the ocean buried. Now are our brows bound with victorious wreaths; our bruised arms hung up for monuments; our stern alarums changed to merry meetings, our dreadful marches to delightful measures. Grim-visaged war hath smooth'd his wrinkled front; and now, instead of mounting barded steeds to fright the souls of fearful adversaries, he capers nimbly in a lady's chamber to the lascivious pleasing of a lute.",
            )
            .build()],
    );

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn generates_no_release_note_when_no_commits() {
    let by_category = HashMap::new();
    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}
