use crate::analyzer::{CategorizedCommits, CommitCategory};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use tera::Value;

pub const TEMPLATE: &str = r#"{%- macro contributors(commit) -%}
{%- if commit.contributors %} ({{ commit.contributors | mention | join(sep=", ") }}){% endif -%}
{%- endmacro contributors -%}

{%- if breaking %}
## Breaking Changes
{%- for commit in breaking %}
- {{ commit.hash }} {{ commit.first_line }}{{ self::contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if features %}
## New Features
{%- for commit in features %}
- {{ commit.hash }} {{ commit.first_line }}{{ self::contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if fixes %}
## Bug Fixes
{%- for commit in fixes %}
- {{ commit.hash }} {{ commit.first_line }}{{ self::contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if dependencies %}
## Dependency Updates
{%- for commit in dependencies %}
- {{ commit.hash }} {{ commit.first_line }}{{ self::contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}

*Generated with [release-note](https://github.com/purpleclay/release-note)*"#;

static NUMBERED_LIST: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+\.\s").unwrap());

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
            .filter_map(|v| v.as_str().map(|s| Value::String(format!("@{}", s))))
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

pub fn render_history(categorized: &CategorizedCommits) -> Result<String> {
    if categorized.by_category.is_empty() {
        return Ok(String::new());
    }

    let mut tera = tera::Tera::default();
    tera.add_raw_template("main", TEMPLATE)
        .context("failed to parse template")?;

    tera.register_filter("unwrap", unwrap_filter);
    tera.register_filter("mention", mention_filter);

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
