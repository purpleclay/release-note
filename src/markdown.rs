use crate::{analyzer::AnalyzedCommits, platform::Platform};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use tera::Value;

static NUMBERED_LIST: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+\.\s").unwrap());
static TABLE_SEPARATOR: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\|[\s\-:|]+\|$").unwrap());

fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();
    (trimmed.starts_with('|') && trimmed.ends_with('|')) || TABLE_SEPARATOR.is_match(trimmed)
}

fn is_indented(line: &str) -> bool {
    line.starts_with("    ") || line.starts_with('\t')
}

fn is_structured_content(para: &str) -> bool {
    para.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || trimmed.starts_with("> ")
            || trimmed.starts_with("```")
            || is_indented(line)
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
        && !is_indented(line)
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

        if is_indented(line) {
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

fn get_lowercase_strings(value: &Value) -> Vec<String> {
    match value {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_lowercase))
            .collect(),
        Value::String(s) => vec![s.to_lowercase()],
        _ => vec![],
    }
}

fn filter_by_field(
    value: &Value,
    args: &HashMap<String, Value>,
    field: &str,
    filter: &str,
) -> tera::Result<Value> {
    let arr = value
        .as_array()
        .ok_or_else(|| tera::Error::msg(format!("{} filter requires an array", filter)))?;

    let include = args
        .get("include")
        .map(get_lowercase_strings)
        .unwrap_or_default();
    let exclude = args
        .get("exclude")
        .map(get_lowercase_strings)
        .unwrap_or_default();

    let filtered: Vec<Value> = arr
        .iter()
        .filter(|item| {
            let field = item
                .get(field)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();

            let included = include.is_empty() || include.contains(&field);
            let excluded = exclude.contains(&field);

            included && !excluded
        })
        .cloned()
        .collect();

    Ok(Value::Array(filtered))
}

fn typed_filter(value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    filter_by_field(value, args, "type", "typed")
}

fn scoped_filter(value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    filter_by_field(value, args, "scope", "scoped")
}

fn table_escape_filter(value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
    let text = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("table_escape filter requires a string value"))?;

    Ok(Value::String(text.replace('|', "\\|")))
}

fn register_platform_functions(tera: &mut tera::Tera, git_ref: &str, platform: &Platform) {
    let platform = platform.clone();

    tera.register_function("commit_url", {
        let platform = platform.clone();
        move |args: &HashMap<String, Value>| -> tera::Result<Value> {
            let sha = args
                .get("sha")
                .and_then(|v| v.as_str())
                .ok_or_else(|| tera::Error::msg("commit_url requires 'sha'"))?;

            let short_sha = &sha[..7.min(sha.len())];

            if let Some(url) = platform.commit_url(sha) {
                Ok(Value::String(format!("[**`{}`**]({})", short_sha, url)))
            } else {
                Ok(Value::String(format!("**`{}`**", short_sha)))
            }
        }
    });

    tera.register_function("contributor_commits_url", {
        let platform = platform.clone();
        let git_ref = git_ref.to_string();
        move |args: &HashMap<String, Value>| -> tera::Result<Value> {
            let author = args.get("author").and_then(|v| v.as_str()).unwrap_or("");
            let since = args.get("since").and_then(|v| v.as_str()).unwrap_or("");
            let until = args.get("until").and_then(|v| v.as_str()).unwrap_or("");

            if let Some(url) = platform.commits_url(&git_ref, author, since, until) {
                Ok(Value::String(url))
            } else {
                Ok(Value::Null)
            }
        }
    });
}

pub fn render_history(
    analyzed: &AnalyzedCommits,
    platform: &Platform,
    git_ref: &str,
    release_date: i64,
    template: &str,
) -> Result<String> {
    if analyzed.commits.is_empty() {
        return Ok(String::new());
    }

    let mut tera = tera::Tera::default();
    tera.add_raw_template("main", template)
        .context("failed to parse template")?;

    tera.register_filter("unwrap", unwrap_filter);
    tera.register_filter("mention", mention_filter);
    tera.register_filter("typed", typed_filter);
    tera.register_filter("scoped", scoped_filter);
    tera.register_filter("table_escape", table_escape_filter);

    register_platform_functions(&mut tera, git_ref, platform);

    let mut context = tera::Context::new();
    context.insert("commits", &analyzed.commits);
    context.insert("contributors", &analyzed.contributors);
    context.insert("git_ref", git_ref);
    context.insert("release_date", &release_date);

    let rendered = tera
        .render("main", &context)
        .context("failed to render template")?;

    Ok(rendered.trim_start().to_string())
}
