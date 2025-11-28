use crate::analyzer::{CategorizedCommits, CommitCategory};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use tera::Value;

pub const TEMPLATE: &str = r#"{%- macro commit_contributors(commit) -%}
{%- if commit.contributors %} ({{ commit.contributors | mention | join(sep=", ") }}){% endif -%}
{%- endmacro commit_contributors -%}

{%- macro contributor_link(contributor, project) -%}
{%- if project -%}
{%- set since = contributor.first_commit_timestamp | date(format="%Y-%m-%d") -%}
{%- set until = contributor.last_commit_timestamp | date(format="%Y-%m-%d") -%}
[**`{{ contributor.count }}`**]({{ project.url }}/commits/{{ project.git_ref }}?author={{ contributor.username }}&since={{ since }}&until={{ until }}) commit{% if contributor.count != 1 %}s{% endif %}
{%- else -%}
{{ contributor.count }} commit{% if contributor.count != 1 %}s{% endif %}
{%- endif -%}
{%- endmacro contributor_link -%}

## {% if project and project.git_ref %}{{ project.git_ref }} - {% endif %}{{ release_date | date(format="%B %d, %Y") }}

{%- set stats = [] -%}
{%- if breaking -%}
  {%- set breaking_count = breaking | length -%}
  {%- if breaking_count > 0 -%}
    {%- if breaking_count == 1 -%}
      {%- set_global stats = stats | concat(with="[**`" ~ breaking_count ~ "`**](#breaking-changes) breaking change") -%}
    {%- else -%}
      {%- set_global stats = stats | concat(with="[**`" ~ breaking_count ~ "`**](#breaking-changes) breaking changes") -%}
    {%- endif -%}
  {%- endif -%}
{%- endif -%}
{%- if features -%}
  {%- set features_count = features | length -%}
  {%- if features_count > 0 -%}
    {%- if features_count == 1 -%}
      {%- set_global stats = stats | concat(with="[**`" ~ features_count ~ "`**](#new-features) new feature") -%}
    {%- else -%}
      {%- set_global stats = stats | concat(with="[**`" ~ features_count ~ "`**](#new-features) new features") -%}
    {%- endif -%}
  {%- endif -%}
{%- endif -%}
{%- if fixes -%}
  {%- set fixes_count = fixes | length -%}
  {%- if fixes_count > 0 -%}
    {%- if fixes_count == 1 -%}
      {%- set_global stats = stats | concat(with="[**`" ~ fixes_count ~ "`**](#bug-fixes) bug fixed") -%}
    {%- else -%}
      {%- set_global stats = stats | concat(with="[**`" ~ fixes_count ~ "`**](#bug-fixes) bug fixes") -%}
    {%- endif -%}
  {%- endif -%}
{%- endif -%}
{%- if stats | length > 0 %}

{{ stats | join(sep=" • ") }}
{% endif %}
{%- if contributors %}
## Contributors
{%- for contributor in contributors | filter(attribute="is_bot", value=false) %}
- <img src="{{ contributor.avatar_url }}&size=20" align="center">&nbsp;&nbsp;@{{ contributor.username }} ({{ self::contributor_link(contributor=contributor, project=project) }})
{%- endfor %}
{% endif %}
{%- if breaking %}
## Breaking Changes
{%- for commit in breaking %}
- {{ commit.hash }} {{ commit.first_line | strip_conventional_prefix }}{{ self::commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if features %}
## New Features
{%- for commit in features %}
- {{ commit.hash }} {{ commit.first_line | strip_conventional_prefix }}{{ self::commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if fixes %}
## Bug Fixes
{%- for commit in fixes %}
- {{ commit.hash }} {{ commit.first_line | strip_conventional_prefix }}{{ self::commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if dependencies %}
{%- set filtered_deps = dependencies | prefix(exclude="chore") %}
{%- if filtered_deps %}
## Dependency Updates
{%- for commit in filtered_deps %}
- {{ commit.hash }} {{ commit.first_line | strip_conventional_prefix }}{{ self::commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- endif %}

*Generated with [release-note](https://github.com/purpleclay/release-note)*"#;

static NUMBERED_LIST: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+\.\s").unwrap());
static TABLE_SEPARATOR: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\|[\s\-:|]+\|$").unwrap());

fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();
    (trimmed.starts_with('|') && trimmed.ends_with('|')) || TABLE_SEPARATOR.is_match(trimmed)
}

fn is_structured_content(para: &str) -> bool {
    para.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || trimmed.starts_with("> ")
            || trimmed.starts_with("```")
            || trimmed.starts_with("    ")
            || trimmed.starts_with('\t')
            || NUMBERED_LIST.is_match(trimmed)
            || is_table_line(line)
    })
}

fn is_continuation_line(line: &str) -> bool {
    let trimmed = line.trim_start();

    !trimmed.starts_with("- ")
        && !trimmed.starts_with("* ")
        && !trimmed.starts_with("+ ")
        && !trimmed.starts_with("> ")
        && !trimmed.starts_with("```")
        && !NUMBERED_LIST.is_match(trimmed)
        && !is_table_line(line)
        && !trimmed.is_empty()
}

fn unwrap_structured_content(para: &str) -> String {
    let mut result = Vec::new();
    let mut current_item = Vec::new();
    let mut in_code_block = false;

    for line in para.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") {
            if !current_item.is_empty() {
                result.push(current_item.join(" "));
                current_item.clear();
            }
            in_code_block = !in_code_block;
            result.push(line.to_string());
            continue;
        }

        if in_code_block {
            result.push(line.to_string());
            continue;
        }

        if is_table_line(line) {
            if !current_item.is_empty() {
                result.push(current_item.join(" "));
                current_item.clear();
            }
            result.push(line.to_string());
            continue;
        }

        if trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || NUMBERED_LIST.is_match(trimmed)
        {
            if !current_item.is_empty() {
                result.push(current_item.join(" "));
                current_item.clear();
            }
            current_item.push(line.to_string());
        } else if is_continuation_line(line) && !current_item.is_empty() {
            current_item.push(trimmed.to_string());
        } else {
            if !current_item.is_empty() {
                result.push(current_item.join(" "));
                current_item.clear();
            }
            result.push(line.to_string());
        }
    }
    if !current_item.is_empty() {
        result.push(current_item.join(" "));
    }

    result.join("\n")
}

fn unwrap_filter(value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
    let text = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("unwrap filter requires a string value"))?;

    let paragraphs: Vec<&str> = text.split("\n\n").collect();

    let unwrapped_paragraphs: Vec<String> = paragraphs
        .iter()
        .map(|para| {
            if para.trim().is_empty() {
                return String::new();
            }

            if para.lines().all(|line| {
                let trimmed = line.trim();
                trimmed.is_empty() || is_table_line(line)
            }) {
                return para.to_string();
            }

            let lines: Vec<&str> = para.lines().collect();
            if lines
                .iter()
                .any(|line| line.trim_start().starts_with("```"))
            {
                para.to_string()
            } else if is_structured_content(para) {
                unwrap_structured_content(para)
            } else {
                let (unfilled, _) = textwrap::unfill(para);
                unfilled
            }
        })
        .collect();

    Ok(Value::String(unwrapped_paragraphs.join("\n\n")))
}

fn mention_filter(value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
    if let Some(arr) = value.as_array() {
        let mentions: Vec<Value> = arr
            .iter()
            .filter_map(|v| {
                if let Some(username) = v.get("username").and_then(|u| u.as_str()) {
                    Some(Value::String(format!("@{}", username)))
                } else {
                    v.as_str().map(|s| Value::String(format!("@{}", s)))
                }
            })
            .collect();
        Ok(Value::Array(mentions))
    } else if let Some(s) = value.as_str() {
        Ok(Value::String(format!("@{}", s)))
    } else {
        Err(tera::Error::msg(
            "mention filter requires a string or array value",
        ))
    }
}

fn get_string_array(value: &Value) -> Vec<String> {
    match value {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        Value::String(s) => vec![s.clone()],
        _ => vec![],
    }
}

fn prefix_filter(value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    let arr = value
        .as_array()
        .ok_or_else(|| tera::Error::msg("prefix filter requires an array"))?;

    let include = args
        .get("include")
        .map(get_string_array)
        .unwrap_or_default();
    let exclude = args
        .get("exclude")
        .map(get_string_array)
        .unwrap_or_default();

    let filtered: Vec<Value> = arr
        .iter()
        .filter(|item| {
            let first_line = item.get("first_line").and_then(Value::as_str).unwrap_or("");

            let included = include.is_empty() || include.iter().any(|p| first_line.starts_with(p));
            let excluded = exclude.iter().any(|p| first_line.starts_with(p));

            included && !excluded
        })
        .cloned()
        .collect();

    Ok(Value::Array(filtered))
}

fn strip_conventional_prefix_filter(
    value: &Value,
    _args: &HashMap<String, Value>,
) -> tera::Result<Value> {
    static CONVENTIONAL_COMMIT_PREFIX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)^[a-z]+(?:\([a-z-]+\))?!?\s*:\s*").unwrap());

    let text = value.as_str().ok_or_else(|| {
        tera::Error::msg("strip_conventional_prefix filter requires a string value")
    })?;

    let stripped = CONVENTIONAL_COMMIT_PREFIX.replace(text, "").to_string();
    Ok(Value::String(stripped))
}

pub fn render_history(
    categorized: &CategorizedCommits,
    project: Option<&crate::metadata::ProjectMetadata>,
    release_date: i64,
) -> Result<String> {
    if categorized.by_category.is_empty() {
        return Ok(String::new());
    }

    let mut tera = tera::Tera::default();
    tera.add_raw_template("main", TEMPLATE)
        .context("failed to parse template")?;

    tera.register_filter("unwrap", unwrap_filter);
    tera.register_filter("mention", mention_filter);
    tera.register_filter("prefix", prefix_filter);
    tera.register_filter(
        "strip_conventional_prefix",
        strip_conventional_prefix_filter,
    );

    let mut context = tera::Context::new();
    context.insert("contributors", &categorized.contributors);
    context.insert("project", &project);
    context.insert("release_date", &release_date);

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
