use crate::{analyzer::AnalyzedCommits, git::ReleaseRange, platform::Platform};
use anyhow::{Context, Result};
use minijinja::value::{Kwargs, Value};
use minijinja::{Environment, Error, ErrorKind, UndefinedBehavior, context};
use once_cell::sync::Lazy;
use regex::Regex;

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

fn unwrap_filter(text: &str) -> String {
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

    unwrapped_paragraphs.join("\n\n")
}

fn mention_filter(value: Value) -> Result<Value, Error> {
    if let Some(s) = value.as_str() {
        return Ok(Value::from(format!("@{}", s)));
    }

    let mentions: Vec<Value> = value
        .try_iter()?
        .filter_map(|v| {
            let username = v.get_attr("username").ok().filter(|u| !u.is_undefined());
            username
                .as_ref()
                .unwrap_or(&v)
                .as_str()
                .map(|s| Value::from(format!("@{}", s)))
        })
        .collect();
    Ok(Value::from(mentions))
}

fn get_lowercase_strings(value: Option<Value>) -> Vec<String> {
    match value {
        Some(v) if v.as_str().is_some() => vec![v.as_str().unwrap().to_lowercase()],
        Some(v) => v
            .try_iter()
            .map(|items| {
                items
                    .filter_map(|item| item.as_str().map(str::to_lowercase))
                    .collect()
            })
            .unwrap_or_default(),
        None => vec![],
    }
}

fn filter_by_field(value: Value, kwargs: Kwargs, field: &str) -> Result<Value, Error> {
    let include = get_lowercase_strings(kwargs.get("include")?);
    let exclude = get_lowercase_strings(kwargs.get("exclude")?);
    kwargs.assert_all_used()?;

    let mut filtered = Vec::new();
    for item in value.try_iter()? {
        let field = item.get_attr(field)?.as_str().unwrap_or("").to_lowercase();

        let included = include.is_empty() || include.contains(&field);
        let excluded = exclude.contains(&field);

        if included && !excluded {
            filtered.push(item);
        }
    }

    Ok(Value::from(filtered))
}

fn typed_filter(value: Value, kwargs: Kwargs) -> Result<Value, Error> {
    filter_by_field(value, kwargs, "type")
}

fn scoped_filter(value: Value, kwargs: Kwargs) -> Result<Value, Error> {
    filter_by_field(value, kwargs, "scope")
}

fn table_escape_filter(text: &str) -> String {
    text.replace('|', "\\|")
}

// Formats a Unix timestamp in UTC using strftime directives, e.g. "%B %d, %Y".
fn date_filter(timestamp: i64, kwargs: Kwargs) -> Result<String, Error> {
    let format: Option<&str> = kwargs.get("format")?;
    kwargs.assert_all_used()?;

    let invalid = |e: jiff::Error| Error::new(ErrorKind::InvalidOperation, e.to_string());
    let timestamp = jiff::Timestamp::from_second(timestamp).map_err(invalid)?;
    jiff::fmt::strtime::format(format.unwrap_or("%Y-%m-%d"), timestamp).map_err(invalid)
}

fn register_platform_functions(
    env: &mut Environment,
    git_ref: &str,
    range: &ReleaseRange,
    platform: &Platform,
) {
    env.add_function("compare_url", {
        let platform = platform.clone();
        let range = range.clone();
        move |kwargs: Kwargs| -> Result<Value, Error> {
            let from: Option<String> = kwargs.get("from")?;
            let to: Option<String> = kwargs.get("to")?;
            kwargs.assert_all_used()?;

            let from = from.unwrap_or_else(|| range.from.clone());
            let to = to.or_else(|| range.to.clone());
            Ok(to
                .and_then(|to| platform.compare_url(&to, &from))
                .map_or(Value::from(()), Value::from))
        }
    });

    env.add_function("commit_url", {
        let platform = platform.clone();
        move |kwargs: Kwargs| -> Result<String, Error> {
            let sha: String = kwargs.get("sha")?;
            kwargs.assert_all_used()?;

            let short_sha = &sha[..7.min(sha.len())];

            Ok(match platform.commit_url(&sha) {
                Some(url) => format!("[**`{}`**]({})", short_sha, url),
                None => format!("**`{}`**", short_sha),
            })
        }
    });

    env.add_function("contributor_commits_url", {
        let platform = platform.clone();
        let git_ref = git_ref.to_string();
        move |kwargs: Kwargs| -> Result<Value, Error> {
            let author: Option<String> = kwargs.get("author")?;
            let since: Option<String> = kwargs.get("since")?;
            let until: Option<String> = kwargs.get("until")?;
            kwargs.assert_all_used()?;

            Ok(platform
                .commits_url(
                    &git_ref,
                    author.as_deref().unwrap_or(""),
                    since.as_deref().unwrap_or(""),
                    until.as_deref().unwrap_or(""),
                )
                .map_or(Value::from(()), Value::from))
        }
    });
}

pub fn render_history(
    analyzed: &AnalyzedCommits,
    platform: &Platform,
    git_ref: &str,
    range: &ReleaseRange,
    release_date: i64,
    template: &str,
) -> Result<String> {
    if analyzed.commits.is_empty() {
        return Ok(String::new());
    }

    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::SemiStrict);

    env.add_filter("unwrap", unwrap_filter);
    env.add_filter("mention", mention_filter);
    env.add_filter("typed", typed_filter);
    env.add_filter("scoped", scoped_filter);
    env.add_filter("table_escape", table_escape_filter);
    env.add_filter("date", date_filter);

    register_platform_functions(&mut env, git_ref, range, platform);

    env.add_template("main", template)
        .context("failed to parse template")?;

    let rendered = env
        .get_template("main")?
        .render(context! {
            commits => &analyzed.commits,
            contributors => &analyzed.contributors,
            git_ref => git_ref,
            from_ref => &range.from,
            to_ref => &range.to,
            release_date => release_date,
        })
        .context("failed to render template")?;

    Ok(rendered.trim_start().to_string())
}
