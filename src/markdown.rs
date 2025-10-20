use crate::analyzer::{CategorizedCommits, CommitCategory};
use anyhow::{Context, Result};

pub const TEMPLATE: &str = r#"{%- if breaking %}
## Breaking Changes
{%- for commit in breaking %}
- {{ commit.hash }} {{ commit.first_line }}
{%- endfor %}

{%- endif %}
{%- if features %}
## New Features
{%- for commit in features %}
- {{ commit.hash }} {{ commit.first_line }}
{%- endfor %}

{%- endif %}
{%- if fixes %}
## Bug Fixes
{%- for commit in fixes %}
- {{ commit.hash }} {{ commit.first_line }}
{%- endfor %}

{%- endif %}
{%- if dependencies %}
## Dependency Updates
{%- for commit in dependencies %}
- {{ commit.hash }} {{ commit.first_line }}
{%- endfor %}

{%- endif %}

*Generated with [release-note](https://github.com/purpleclay/release-note)*"#;

pub fn render_history(categorized: &CategorizedCommits) -> Result<String> {
    let mut tera = tera::Tera::default();
    tera.add_raw_template("main", TEMPLATE)
        .context("failed to parse template")?;

    let mut context = tera::Context::new();

    if let Some(breaking) = categorized.by_category.get(&CommitCategory::Breaking) {
        context.insert("breaking", breaking);
    }
    if let Some(features) = categorized.by_category.get(&CommitCategory::Feature) {
        context.insert("features", features);
    }
    if let Some(fixes) = categorized.by_category.get(&CommitCategory::Fix) {
        context.insert("fixes", fixes);
    }
    if let Some(dependencies) = categorized.by_category.get(&CommitCategory::Dependencies) {
        context.insert("dependencies", dependencies);
    }

    let rendered = tera
        .render("main", &context)
        .context("failed to render template")?;

    Ok(rendered.trim_start().to_string())
}
