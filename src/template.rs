use anyhow::{Context, Result};
use minijinja::Environment;
use std::path::PathBuf;

pub const DEFAULT_TEMPLATE: &str = r#"{%- macro commit_contributors(commit) -%}
{%- if commit.contributors %} ({{ commit.contributors | mention | join(", ") }}){% endif -%}
{%- endmacro -%}

{%- macro contributor_link(contributor) -%}
{%- if contributor.is_ai -%}
**`{{ contributor.count }}`** commit{% if contributor.count != 1 %}s{% endif %}
{%- else -%}
{%- set since = contributor.first_commit_timestamp | date(format="%Y-%m-%d") -%}
{%- set until = contributor.last_commit_timestamp | date(format="%Y-%m-%d") -%}
{%- set url = contributor_commits_url(author=contributor.username, since=since, until=until) -%}
{%- if url -%}
[**`{{ contributor.count }}`**]({{ url }}) commit{% if contributor.count != 1 %}s{% endif %}
{%- else -%}
**`{{ contributor.count }}`** commit{% if contributor.count != 1 %}s{% endif %}
{%- endif -%}
{%- endif -%}
{%- endmacro -%}

{%- macro avatar_url(url, size) -%}
{{ url }}{% if "?" in url %}&{% else %}?{% endif %}size={{ size }}&width={{ size }}
{%- endmacro -%}

{%- set breaking = commits | selectattr("breaking") -%}
{%- set non_breaking = commits | rejectattr("breaking") -%}
{%- set dependencies = non_breaking | scoped(include="deps") -%}
{%- set changes = non_breaking | scoped(exclude="deps") -%}
{%- set features = changes | typed(include="feat") -%}
{%- set fixes = changes | typed(include="fix") -%}
{%- set perf = changes | typed(include="perf") -%}

## {{ git_ref }} - {{ release_date | date(format="%B %d, %Y") }}

{%- set stats = [] -%}
{%- if breaking -%}
  {%- set breaking_count = breaking | length -%}
  {%- if breaking_count > 0 -%}
    {%- if breaking_count == 1 -%}
      {%- set stats = stats + ["[**`" ~ breaking_count ~ "`**](#breaking-changes) breaking change"] -%}
    {%- else -%}
      {%- set stats = stats + ["[**`" ~ breaking_count ~ "`**](#breaking-changes) breaking changes"] -%}
    {%- endif -%}
  {%- endif -%}
{%- endif -%}
{%- if features -%}
  {%- set features_count = features | length -%}
  {%- if features_count > 0 -%}
    {%- if features_count == 1 -%}
      {%- set stats = stats + ["[**`" ~ features_count ~ "`**](#new-features) new feature"] -%}
    {%- else -%}
      {%- set stats = stats + ["[**`" ~ features_count ~ "`**](#new-features) new features"] -%}
    {%- endif -%}
  {%- endif -%}
{%- endif -%}
{%- if fixes -%}
  {%- set fixes_count = fixes | length -%}
  {%- if fixes_count > 0 -%}
    {%- if fixes_count == 1 -%}
      {%- set stats = stats + ["[**`" ~ fixes_count ~ "`**](#bug-fixes) bug fixed"] -%}
    {%- else -%}
      {%- set stats = stats + ["[**`" ~ fixes_count ~ "`**](#bug-fixes) bug fixes"] -%}
    {%- endif -%}
  {%- endif -%}
{%- endif -%}
{%- if stats | length > 0 %}

{{ stats | join(" • ") }}
{% endif %}
{%- set humans = contributors | rejectattr("is_bot") -%}
{%- if humans %}
## Contributors
{%- for contributor in humans %}
- <img src="{{ avatar_url(url=contributor.avatar_url, size=20) }}" width="20" height="20" align="center">&nbsp;&nbsp;@{{ contributor.username }} ({{ contributor_link(contributor=contributor) }})
{%- endfor %}
{% endif %}
{%- if breaking %}
## Breaking Changes
{%- for commit in breaking %}
- {{ commit_url(sha = commit.hash) }} {{ commit.description }}{{ commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(2, first=true) }}
{%- endif %}
{%- if commit.breaking_description %}

> [!IMPORTANT]
{{ "> " ~ (commit.breaking_description | unwrap) | replace("\n", "\n> ") }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if features %}
## New Features
{%- for commit in features %}
- {{ commit_url(sha = commit.hash) }} {{ commit.description }}{{ commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(2, first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if fixes %}
## Bug Fixes
{%- for commit in fixes %}
- {{ commit_url(sha = commit.hash) }} {{ commit.description }}{{ commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(2, first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if perf %}
## Performance Improvements
{%- for commit in perf %}
- {{ commit_url(sha = commit.hash) }} {{ commit.description }}{{ commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(2, first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if dependencies %}
## Dependency Updates

| Commit | Update | Contributors |
|--------|--------|--------------|
{%- for commit in dependencies %}
| {{ commit_url(sha = commit.hash) }} | {{ commit.description | table_escape }} |{% if commit.contributors %} {{ commit.contributors | mention | join(", ") }}{% endif %} |
{%- endfor %}

{%- endif %}
{%- set compare = compare_url() %}
{%- if compare %}

**Full Changelog**: [`{{ to_ref }}...{{ from_ref }}`]({{ compare }})
{%- endif %}

*Generated with [release-note](https://github.com/purpleclay/release-note)*"#;

pub struct TemplateResolver {
    working_dir: PathBuf,
}

impl TemplateResolver {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    pub fn resolve(&self) -> Result<String> {
        let candidates = [
            self.working_dir.join("release-note.jinja"),
            self.working_dir.join(".github/release-note.jinja"),
            self.working_dir.join(".gitlab/release-note.jinja"),
        ];

        for path in candidates {
            if path.is_file() {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read template: {}", path.display()))?;

                Environment::new()
                    .template_from_named_str(&path.display().to_string(), &content)
                    .with_context(|| format!("invalid template syntax in {}", path.display()))?;

                log::info!("using custom template: {}", path.display());
                return Ok(content);
            }

            // Tera templates are no longer supported. Fail rather than fall back to
            // a lower-priority or default template
            let tera = path.with_extension("tera");
            if tera.is_file() {
                anyhow::bail!(
                    "found Tera template {}, templates now use Jinja syntax and must be renamed to release-note.jinja",
                    tera.display()
                );
            }
        }

        Ok(DEFAULT_TEMPLATE.to_string())
    }
}
