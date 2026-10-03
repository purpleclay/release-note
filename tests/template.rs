use release_note::template::{DEFAULT_TEMPLATE, TemplateResolver};
use std::fs;
use tempfile::TempDir;

#[test]
fn uses_template_from_root_as_highest_priority() {
    let temp_dir = TempDir::new().unwrap();
    let root_template = "# Root template";
    let github_template = "# GitHub template";

    fs::write(temp_dir.path().join("release-note.jinja"), root_template).unwrap();
    fs::create_dir_all(temp_dir.path().join(".github")).unwrap();
    fs::write(
        temp_dir.path().join(".github/release-note.jinja"),
        github_template,
    )
    .unwrap();

    let resolver = TemplateResolver::new(temp_dir.path().to_path_buf());
    let template = resolver.resolve().unwrap();

    assert_eq!(template, root_template);
}

#[test]
fn uses_template_from_github_directory() {
    let temp_dir = TempDir::new().unwrap();
    let github_template = "# GitHub template";

    fs::create_dir_all(temp_dir.path().join(".github")).unwrap();
    fs::write(
        temp_dir.path().join(".github/release-note.jinja"),
        github_template,
    )
    .unwrap();

    let resolver = TemplateResolver::new(temp_dir.path().to_path_buf());
    let template = resolver.resolve().unwrap();

    assert_eq!(template, github_template);
}

#[test]
fn uses_template_from_gitlab_directory() {
    let temp_dir = TempDir::new().unwrap();
    let gitlab_template = "# GitLab template";

    fs::create_dir_all(temp_dir.path().join(".gitlab")).unwrap();
    fs::write(
        temp_dir.path().join(".gitlab/release-note.jinja"),
        gitlab_template,
    )
    .unwrap();

    let resolver = TemplateResolver::new(temp_dir.path().to_path_buf());
    let template = resolver.resolve().unwrap();

    assert_eq!(template, gitlab_template);
}

#[test]
fn falls_back_to_default_template_when_no_custom_template_exists() {
    let temp_dir = TempDir::new().unwrap();

    let resolver = TemplateResolver::new(temp_dir.path().to_path_buf());
    let template = resolver.resolve().unwrap();

    assert_eq!(template, DEFAULT_TEMPLATE);
}

#[test]
fn fails_on_template_with_syntax_errors() {
    let temp_dir = TempDir::new().unwrap();
    let invalid_template = "{{ invalid syntax";

    fs::write(temp_dir.path().join("release-note.jinja"), invalid_template).unwrap();

    let resolver = TemplateResolver::new(temp_dir.path().to_path_buf());
    let result = resolver.resolve();

    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("invalid template syntax"));
}

#[test]
fn fails_on_templates_with_the_tera_extension() {
    let temp_dir = TempDir::new().unwrap();
    fs::create_dir(temp_dir.path().join(".github")).unwrap();
    fs::write(
        temp_dir.path().join(".github/release-note.tera"),
        "# Tera template",
    )
    .unwrap();

    let resolver = TemplateResolver::new(temp_dir.path().to_path_buf());
    let error = format!("{:#}", resolver.resolve().unwrap_err());

    assert!(error.contains(".github/release-note.tera"), "{error}");
    assert!(error.contains("release-note.jinja"), "{error}");
}

#[test]
fn prefers_jinja_template_over_tera_template() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("release-note.tera"), "# Tera template").unwrap();
    fs::write(
        temp_dir.path().join("release-note.jinja"),
        "# Jinja template",
    )
    .unwrap();

    let resolver = TemplateResolver::new(temp_dir.path().to_path_buf());
    let template = resolver.resolve().unwrap();

    assert_eq!(template, "# Jinja template");
}

#[test]
fn fails_on_tera_template_with_higher_priority_than_jinja_template() {
    let temp_dir = TempDir::new().unwrap();
    fs::create_dir(temp_dir.path().join(".github")).unwrap();
    fs::write(temp_dir.path().join("release-note.tera"), "# Tera template").unwrap();
    fs::write(
        temp_dir.path().join(".github/release-note.jinja"),
        "# Jinja template",
    )
    .unwrap();

    let resolver = TemplateResolver::new(temp_dir.path().to_path_buf());
    let error = format!("{:#}", resolver.resolve().unwrap_err());

    assert!(error.contains("release-note.tera"), "{error}");
}

#[test]
fn reports_the_line_of_a_template_syntax_error() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("release-note.jinja"),
        "## Release\n\n{% endif %}",
    )
    .unwrap();

    let resolver = TemplateResolver::new(temp_dir.path().to_path_buf());
    let error = format!("{:#}", resolver.resolve().unwrap_err());

    assert!(error.contains("invalid template syntax"), "{error}");
    assert!(error.contains("release-note.jinja:3"), "{error}");
}
