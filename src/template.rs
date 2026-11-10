use anyhow::{Context, Result};
use minijinja::machinery::{Token, tokenize};
use minijinja::syntax::SyntaxConfig;
use std::path::{Path, PathBuf};

pub const DEFAULT_TEMPLATE: &str = include_str!("templates/default.jinja");

// Templates a custom template can extend, import or include, by name
pub(crate) const BUILT_IN_TEMPLATES: [(&str, &str); 1] =
    [("release-note/default", DEFAULT_TEMPLATE)];

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

                crate::markdown::environment()
                    .template_from_named_str(&path.display().to_string(), &content)
                    .with_context(|| format!("invalid template syntax in {}", path.display()))?;
                check_template_references(&path, &content)?;

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

// minijinja only resolves template names when rendering, which happens after the
// git and network work, so a misspelled name is caught here instead. Tokens rather
// than text are checked, so strings, comments and raw blocks never count as a tag
fn check_template_references(path: &Path, content: &str) -> Result<()> {
    let tokens: Vec<Token> = tokenize(content, false, SyntaxConfig::default())
        .map(|token| token.map(|(token, _)| token))
        .collect::<Result<_, _>>()?;

    for (start, tag) in tokens.windows(4).enumerate() {
        let [Token::BlockStart, Token::Ident(keyword), name, next] = tag else {
            continue;
        };
        if !matches!(*keyword, "extends" | "import" | "from" | "include") {
            continue;
        }
        let name = match name {
            Token::Str(name) => *name,
            Token::String(name) => name,
            _ => continue,
        };
        // shortcut: a name built from an expression, such as "a" ~ b or a list passed
        // to include, is left for render time to report; check it here if templates
        // start composing names
        let literal = matches!(next, Token::BlockEnd)
            || matches!(
                next,
                Token::Ident("as" | "import" | "ignore" | "with" | "without")
            );
        // An include marked "ignore missing" renders nothing when the template is
        // missing, and the marker can follow "with context", so search the whole tag
        let optional = *keyword == "include"
            && tokens[start + 3..]
                .iter()
                .take_while(|token| !matches!(token, Token::BlockEnd))
                .any(|token| matches!(token, Token::Ident("ignore")));
        if !literal
            || optional
            || BUILT_IN_TEMPLATES
                .iter()
                .any(|(built_in, _)| *built_in == name)
        {
            continue;
        }

        let built_ins: Vec<&str> = BUILT_IN_TEMPLATES.iter().map(|(n, _)| *n).collect();
        anyhow::bail!(
            "unknown template '{}' in {}, built-in templates are: {}",
            name,
            path.display(),
            built_ins.join(", ")
        );
    }
    Ok(())
}
