use anyhow::Result;
use clap::{Parser, arg};
use release_note::platform::Platform;
use std::path::PathBuf;

use release_note::analyzer::CommitAnalyzer;
use release_note::contributor;
use release_note::git::{GitRepo, History};
use release_note::markdown;
use release_note::template::TemplateResolver;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, disable_version_flag = true, disable_help_subcommand = true)]
struct Args {
    /// The commits to include, using git's range syntax. Defaults to HEAD.
    ///
    /// Examples:
    ///  - v1.0.0          v1.0.0 back to the previous release (detected).
    ///  - v0.9.0..v1.0.0  commits after v0.9.0, up to and including v1.0.0.
    ///  - v0.9.0..        commits after v0.9.0, up to and including HEAD.
    ///
    /// Either side can be a commit hash, a tag, a branch or a relative
    /// reference such as HEAD~3. An empty side means HEAD.
    #[arg(value_name = "RANGE", verbatim_doc_comment)]
    range: Option<String>,

    /// Path to a directory within the repository.
    ///
    /// Can be:
    ///  - Repository root (default: ".") - shows all commits.
    ///  - A subdirectory (e.g., "ui/") - filters commits to only those affecting that directory.
    #[arg(value_name = "DIR", long, default_value = ".", verbatim_doc_comment)]
    path: PathBuf,

    /// Trust a host for token attachment (e.g. a self-hosted GitHub Enterprise or GitLab
    /// instance). Can be repeated or comma-separated. Without this flag, tokens are only
    /// sent to github.com, *.github.com, and gitlab.com.
    #[arg(
        long,
        value_name = "HOST",
        value_delimiter = ',',
        env = "RELEASE_NOTE_TRUSTED_HOST"
    )]
    trusted_host: Vec<String>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Print build time version information
    #[arg(short = 'V', long)]
    version: bool,
}

// Prints each message to stderr without a level or timestamp
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        eprintln!("{}", record.args());
    }

    fn flush(&self) {}
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.version {
        print_version_info();
        return Ok(());
    }

    if args.verbose {
        log::set_logger(&StderrLogger).expect("logger is only set once");
        log::set_max_level(log::LevelFilter::Info);
    }

    let template = TemplateResolver::new(args.path.clone()).resolve()?;

    let (from, to) = split_range(args.range.as_deref())?;
    let repo = GitRepo::open(&args.path)?;
    let History {
        range,
        commits: mut history,
    } = repo.history(from.clone(), to)?;

    let git_ref = from.unwrap_or_else(|| range.from.clone());
    let platform = Platform::detect(repo.origin_url(), &args.trusted_host);

    if let Ok(Some(mut resolver)) = contributor::ContributorResolver::new(&platform) {
        resolver.resolve_contributors(&mut history);
    }

    let analyzed = CommitAnalyzer::analyze(&history);
    log::info!("");

    let release_date = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    println!(
        "{}",
        markdown::render_history(
            &analyzed,
            &platform,
            &git_ref,
            &range,
            release_date,
            &template
        )?
    );
    Ok(())
}

// Splits a git range `old..new` into the refs history() takes, newest first.
// An empty `new` is left unset, so the release is named after what HEAD resolves to
fn split_range(range: Option<&str>) -> Result<(Option<String>, Option<String>)> {
    let Some(range) = range else {
        return Ok((None, None));
    };
    if range.contains("...") {
        anyhow::bail!(
            "symmetric difference ranges are not supported, did you mean {}?",
            range.replacen("...", "..", 1)
        );
    }
    let Some((old, new)) = range.split_once("..") else {
        return Ok((Some(range.to_string()), None));
    };

    let new = (!new.is_empty()).then(|| new.to_string());
    let old = if old.is_empty() { "HEAD" } else { old };
    Ok((new, Some(old.to_string())))
}

fn print_version_info() {
    let fields = [
        ("version", Some(env!("CARGO_PKG_VERSION"))),
        ("rustc", option_env!("RELEASE_NOTE_RUSTC")),
        ("target", option_env!("RELEASE_NOTE_TARGET")),
        ("git_branch", option_env!("RELEASE_NOTE_GIT_REF")),
        ("git_commit", option_env!("RELEASE_NOTE_GIT_SHA")),
        ("commit_date", option_env!("RELEASE_NOTE_COMMIT_DATE")),
    ];
    for (name, value) in fields {
        if let Some(value) = value {
            println!("{:<12} {value}", format!("{name}:"));
        }
    }
}
