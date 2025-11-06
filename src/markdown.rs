use crate::analyzer::{CategorizedCommits, CommitCategory};
use anyhow::{Context, Result};
use std::collections::HashMap;
use tera::Value;

pub const TEMPLATE: &str = r#"{%- if breaking %}
## Breaking Changes
{%- for commit in breaking %}
- {{ commit.hash }} {{ commit.first_line }}{% if commit.contributor %} (@{{ commit.contributor }}){% endif %}
{%- if commit.body %}

{{ commit.body | wrap(width=80) | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if features %}
## New Features
{%- for commit in features %}
- {{ commit.hash }} {{ commit.first_line }}{% if commit.contributor %} (@{{ commit.contributor }}){% endif %}
{%- if commit.body %}

{{ commit.body | wrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if fixes %}
## Bug Fixes
{%- for commit in fixes %}
- {{ commit.hash }} {{ commit.first_line }}{% if commit.contributor %} (@{{ commit.contributor }}){% endif %}
{%- if commit.body %}

{{ commit.body | wrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if dependencies %}
## Dependency Updates
{%- for commit in dependencies %}
- {{ commit.hash }} {{ commit.first_line }}{% if commit.contributor %} (@{{ commit.contributor }}){% endif %}
{%- if commit.body %}

{{ commit.body | wrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}

*Generated with [release-note](https://github.com/purpleclay/release-note)*"#;

fn wrap_filter(value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    let text = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("wrap filter requires a string value"))?;

    let width = args.get("width").and_then(|v| v.as_u64()).unwrap_or(80) as usize;

    let (unwrapped, _) = textwrap::unfill(text);

    let options = textwrap::Options::new(width);
    let wrapped = textwrap::fill(&unwrapped, options);
    Ok(Value::String(wrapped))
}

pub fn render_history(categorized: &CategorizedCommits) -> Result<String> {
    if categorized.by_category.is_empty() {
        return Ok(String::new());
    }

    let mut tera = tera::Tera::default();
    tera.add_raw_template("main", TEMPLATE)
        .context("failed to parse template")?;

    tera.register_filter("wrap", wrap_filter);

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
