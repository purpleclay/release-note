use crate::analyzer::{CategorizedCommits, CommitCategory};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use tera::Value;

static NUMBERED_LIST: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+\.\s").unwrap());

static MARKDOWN_LINK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap() // [text](url)
});
static MARKDOWN_REF_LINK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[([^\]]+)\]\[([^\]]*)\]").unwrap() // [text][ref]
});
static MARKDOWN_AUTOLINK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"<((?:https?|ftp|mailto):[^>]+)>").unwrap() // <uxrl>
});
static INLINE_CODE: Lazy<Regex> = Lazy::new(|| {
    // Match inline code: single backtick, content, single backtick
    // But ensure we don't match code blocks (```) by checking boundaries
    // We'll handle code block exclusion in the protect function
    Regex::new(r"(?s)`([^`]+)`").unwrap()
});
static BARE_URL: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://[^\s]+").unwrap());

const NBSP: char = '\u{00A0}';

fn create_placeholder(display_width: usize, counter: usize) -> String {
    let base = format!("__P{}__", counter);
    if base.len() < display_width {
        format!("{}{}", base, "_".repeat(display_width - base.len()))
    } else {
        base
    }
}

fn needs_protection(text: &str) -> bool {
    INLINE_CODE.is_match(text)
        || MARKDOWN_LINK.is_match(text)
        || MARKDOWN_REF_LINK.is_match(text)
        || MARKDOWN_AUTOLINK.is_match(text)
        || BARE_URL.is_match(text)
}

fn protect_markdown_elements(text: &str) -> (Cow<'_, str>, HashMap<String, String>) {
    if !needs_protection(text) {
        return (Cow::Borrowed(text), HashMap::new());
    }

    let mut replacements = HashMap::new();
    let mut counter = 0;

    // Protect inline code blocks first
    // If inline code spans multiple lines, join it into one line
    // We need to collect matches first to avoid issues with replacing while iterating
    let inline_code_matches: Vec<_> = INLINE_CODE
        .captures_iter(text)
        .filter_map(|cap| {
            let match_obj = cap.get(0).unwrap();
            let full_match = match_obj.as_str();
            let match_start = match_obj.start();

            let before = if match_start > 0 {
                text.chars().nth(match_start - 1)
            } else {
                None
            };
            let after = text.chars().nth(match_obj.end());

            if before == Some('`') || after == Some('`') {
                return None;
            }

            let code_content = cap.get(1).unwrap().as_str();
            let normalized = if code_content.contains('\n') {
                format!("`{}`", code_content.replace('\n', " ").trim())
            } else {
                full_match.to_string()
            };

            // Replace spaces with non-breaking spaces to break textwrap from splitting it
            Some((
                full_match.to_string(),
                normalized.replace(' ', &NBSP.to_string()),
            ))
        })
        .collect();

    let mut protected = text.to_string();

    for (original, replacement) in inline_code_matches {
        protected = protected.replace(&original, &replacement);
    }

    for cap in MARKDOWN_LINK.captures_iter(&protected.clone()) {
        let full_match = cap.get(0).unwrap().as_str();
        let link_text = cap.get(1).unwrap().as_str();

        let placeholder = create_placeholder(link_text.len(), counter);
        counter += 1;
        replacements.insert(placeholder.clone(), full_match.to_string());
        protected = protected.replace(full_match, &placeholder);
    }

    for cap in MARKDOWN_REF_LINK.captures_iter(&protected.clone()) {
        let full_match = cap.get(0).unwrap().as_str();
        let link_text = cap.get(1).unwrap().as_str();

        let placeholder = create_placeholder(link_text.len(), counter);
        counter += 1;
        replacements.insert(placeholder.clone(), full_match.to_string());
        protected = protected.replace(full_match, &placeholder);
    }

    for cap in MARKDOWN_AUTOLINK.captures_iter(&protected.clone()) {
        let full_match = cap.get(0).unwrap().as_str();
        let url = cap.get(1).unwrap().as_str();

        let placeholder = create_placeholder(url.len(), counter);
        counter += 1;
        replacements.insert(placeholder.clone(), full_match.to_string());
        protected = protected.replace(full_match, &placeholder);
    }

    // Protect bare URLs with placeholders (not already protected)
    // Display width is the full URL length
    for cap in BARE_URL.captures_iter(&protected.clone()) {
        let full_match = cap.get(0).unwrap().as_str();
        // Skip if this looks like it's already a placeholder
        if !full_match.starts_with("__P") {
            let placeholder = create_placeholder(full_match.len(), counter);
            counter += 1;
            replacements.insert(placeholder.clone(), full_match.to_string());
            protected = protected.replace(full_match, &placeholder);
        }
    }

    (Cow::Owned(protected), replacements)
}

fn restore_markdown_elements(text: &str, replacements: &HashMap<String, String>) -> String {
    let mut result = text.replace(NBSP, " ");
    for (placeholder, original) in replacements {
        result = result.replace(placeholder, original);
    }
    result
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
    })
}

fn wrap_structured_content(para: &str, base_options: &textwrap::Options) -> String {
    let mut result = Vec::new();
    let mut in_code_block = false;

    for line in para.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            result.push(line.to_string());
            continue;
        }

        if in_code_block || trimmed.starts_with("    ") || trimmed.starts_with('\t') {
            result.push(line.to_string());
        } else if trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
        {
            let indent_size = line.len() - trimmed.len() + 2;
            let indent = " ".repeat(indent_size);
            let options = base_options.clone().subsequent_indent(&indent);
            result.push(textwrap::fill(line, options));
        } else if let Some(captures) = NUMBERED_LIST.captures(trimmed) {
            let prefix_len = captures.get(0).unwrap().as_str().len();
            let indent_size = line.len() - trimmed.len() + prefix_len;
            let indent = " ".repeat(indent_size);
            let options = base_options.clone().subsequent_indent(&indent);
            result.push(textwrap::fill(line, options));
        } else if trimmed.starts_with("> ") {
            let base_indent = " ".repeat(line.len() - trimmed.len());
            let indent = format!("{}> ", base_indent);
            let options = base_options.clone().subsequent_indent(&indent);
            result.push(textwrap::fill(line, options));
        } else if line.trim().is_empty() {
            result.push(String::new());
        } else {
            result.push(textwrap::fill(line, base_options.clone()));
        }
    }

    result.join("\n")
}

pub const TEMPLATE: &str = r#"{%- if breaking %}
## Breaking Changes
{%- for commit in breaking %}
- {{ commit.hash }} {{ commit.first_line }}{% if commit.contributor %} (@{{ commit.contributor }}){% endif %}
{%- if commit.body %}

{{ commit.body | wrap | indent(prefix = "  ", first=true) }}
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

    let base_options = textwrap::Options::new(width)
        .break_words(false)
        .wrap_algorithm(textwrap::WrapAlgorithm::OptimalFit(
            textwrap::wrap_algorithms::Penalties::default(),
        ));

    let paragraphs: Vec<&str> = text.split("\n\n").collect();

    let wrapped_paragraphs: Vec<String> = paragraphs
        .iter()
        .map(|para| {
            if para.trim().is_empty() {
                return String::new();
            }

            if is_structured_content(para) {
                let (protected, replacements) = protect_markdown_elements(para);
                let wrapped = wrap_structured_content(&protected, &base_options);
                restore_markdown_elements(&wrapped, &replacements)
            } else {
                let (protected, replacements) = protect_markdown_elements(para);
                let (unwrapped, _) = textwrap::unfill(&protected);
                let wrapped = textwrap::fill(&unwrapped, &base_options);
                restore_markdown_elements(&wrapped, &replacements)
            }
        })
        .collect();

    Ok(Value::String(wrapped_paragraphs.join("\n\n")))
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
