use crate::git::Commit;
use anyhow::{Context, Result};

pub const TEMPLATE: &str = r#"
## What's Changed
{%- for commit in commits %}
- {{ commit.hash }} {{ commit.message }}
{%- endfor %}

*Generated with [release-note](https://github.com/purpleclay/release-note)*
"#;

pub fn render_history(commits: &[Commit]) -> Result<String> {
    let mut tera = tera::Tera::default();
    tera.add_raw_template("main", TEMPLATE)
        .context("failed to parse template")?;

    let mut context = tera::Context::new();
    context.insert("commits", commits);

    tera.render("main", &context)
        .context("failed to render template")
}
