use anyhow::{Context, Result};
use std::path::PathBuf;

pub const DEFAULT_TEMPLATE: &str = r#"{%- macro commit_contributors(commit) -%}
{%- if commit.contributors %} ({{ commit.contributors | mention | join(sep=", ") }}){% endif -%}
{%- endmacro commit_contributors -%}

{%- macro contributor_link(contributor) -%}
{%- set since = contributor.first_commit_timestamp | date(format="%Y-%m-%d") -%}
{%- set until = contributor.last_commit_timestamp | date(format="%Y-%m-%d") -%}
{%- set url = contributor_commits_url(author=contributor.username, since=since, until=until) -%}
{%- if url -%}
[**`{{ contributor.count }}`**]({{ url }}) commit{% if contributor.count != 1 %}s{% endif %}
{%- else -%}
{{ contributor.count }} commit{% if contributor.count != 1 %}s{% endif %}
{%- endif -%}
{%- endmacro contributor_link -%}

## {{ git_ref }} - {{ release_date | date(format="%B %d, %Y") }}

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
- <img src="{{ contributor.avatar_url }}&size=20" align="center">&nbsp;&nbsp;@{{ contributor.username }} ({{ self::contributor_link(contributor=contributor) }})
{%- endfor %}
{% endif %}
{%- if breaking %}
## Breaking Changes
{%- for commit in breaking %}
- {{ commit_url(sha = commit.hash) }} {{ commit.first_line | strip_conventional_prefix }}{{ self::commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if features %}
## New Features
{%- for commit in features %}
- {{ commit_url(sha = commit.hash) }} {{ commit.first_line | strip_conventional_prefix }}{{ self::commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
{%- if fixes %}
## Bug Fixes
{%- for commit in fixes %}
- {{ commit_url(sha = commit.hash) }} {{ commit.first_line | strip_conventional_prefix }}{{ self::commit_contributors(commit=commit) }}
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
- {{ commit_url(sha = commit.hash) }} {{ commit.first_line | strip_conventional_prefix }}{{ self::commit_contributors(commit=commit) }}
{%- if commit.body %}

{{ commit.body | unwrap | indent(prefix = "  ", first=true) }}
{%- endif %}
{%- endfor %}

{%- endif %}
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
            self.working_dir.join("release-note.tera"),
            self.working_dir.join(".github/release-note.tera"),
            self.working_dir.join(".gitlab/release-note.tera"),
        ];

        for path in candidates {
            if path.is_file() {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read template: {}", path.display()))?;

                let mut tera = tera::Tera::default();
                tera.add_raw_template("custom", &content)
                    .with_context(|| format!("invalid template syntax in {}", path.display()))?;

                log::info!("using custom template: {}", path.display());
                return Ok(content);
            }
        }

        Ok(DEFAULT_TEMPLATE.to_string())
    }
}
