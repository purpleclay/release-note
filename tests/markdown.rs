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
fn unfills_paragraph_to_desired_width() {
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

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn preserves_hyphenated_words() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: the ill-fated star-crossed lovers")
            .with_body(
                "In fair Verona, where we lay our scene, the never-ending blood-feud between two well-respected households breaks into new mutiny. The star-crossed lovers, fortune-favoured in their passion yet ill-fated in their destiny, take their life. Their death-marked love and parents' rage could naught but with their children's end bury their strife.",
            )
            .build()],
    );

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn wraps_list_items_with_hanging_indent() {
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

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn wraps_numbered_lists_with_hanging_indent() {
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

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

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

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn wraps_block_quotes_with_quote_markers() {
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

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn preserves_inline_code_without_wrapping() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: preserve formatting in dramatic text")
            .with_body(
                "When Hamlet ponders mortality, he invokes `toBeOrNotToBeThatIsTheQuestion` near the line boundary. Similarly, the method `whetherTisNoblerInTheMindToSufferTheSlingsAndArrows` tests wrapping. Call `theSeaOfTroubles` for reference.",
            )
            .build()],
    );

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}

#[test]
fn preserves_urls_without_wrapping() {
    let mut by_category = HashMap::new();

    by_category.insert(
        CommitCategory::Feature,
        vec![CommitBuilder::new("feat: add references to theatrical works")
            .with_body(
                "For the complete text of Hamlet's soliloquy, see [the full soliloquy](https://www.shakespeare.example/hamlet/act-3/scene-1/soliloquy-to-be-or-not-to-be-that-is-the-question) which contains detailed analysis. Additional commentary available at https://www.globe-theatre.example/analysis/existential-questions-in-elizabethan-drama in our reference library.",
            )
            .build()],
    );

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

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

    let categorized = CategorizedCommits { by_category };
    let result = markdown::render_history(&categorized).unwrap();

    insta::assert_snapshot!(result);
}
