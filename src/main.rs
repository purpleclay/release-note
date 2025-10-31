use anyhow::Result;
use clap::{Parser, arg};
use std::path::PathBuf;

use release_note::analyzer::CommitAnalyzer;
use release_note::git::GitRepo;
use release_note::markdown;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, disable_version_flag = true, disable_help_subcommand = true)]
struct Args {
    /// A starting reference within the git history (inclusive). Defaults to HEAD.
    ///
    /// A reference can be:
    ///  - A commit hash (full or abbreviated).
    ///  - A tag (1.0.0 or refs/tags/1.0.0).
    ///  - A branch name (local or remote).
    ///  - Or a relative reference (HEAD, HEAD~3).
    #[arg(value_name = "FROM", required = false, verbatim_doc_comment)]
    from: Option<String>,

    /// An end reference within the git history (exclusive). TO is excluded from the output.
    /// Supports the same references as FROM.
    #[arg(value_name = "TO", required = false, verbatim_doc_comment)]
    to: Option<String>,

    /// Path to a directory within the repository.
    ///
    /// Can be:
    ///  - Repository root (default: ".") - shows all commits.
    ///  - A subdirectory (e.g., "ui/") - filters commits to only those affecting that directory.
    #[arg(value_name = "DIR", long, default_value = ".", verbatim_doc_comment)]
    path: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Print build time version information
    #[arg(short = 'V', long)]
    version: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.version {
        print_version_info();
        return Ok(());
    }

    if args.verbose {
        env_logger::Builder::new()
            .format(|buf, record| {
                use std::io::Write;
                writeln!(buf, "{}", record.args())
            })
            .filter_level(log::LevelFilter::Info)
            .init();
    }

    let repo = GitRepo::open(&args.path)?;

    let history = repo.history(args.from, args.to)?;
    let categorized = CommitAnalyzer::analyze(&history);
    log::info!("");

    println!("{}", markdown::render_history(&categorized)?);
    Ok(())
}

fn print_version_info() {
    println!("version:    {}", built_info::PKG_VERSION);
    println!("rustc:      {}", built_info::RUSTC_VERSION);
    println!("target:     {}", built_info::TARGET);

    if let Some(git_ref) = built_info::GIT_HEAD_REF {
        println!(
            "git_branch: {}",
            git_ref.strip_prefix("refs/heads/").unwrap_or(git_ref)
        );
    }

    if let Some(commit_hash) = built_info::GIT_COMMIT_HASH {
        println!("git_commit: {commit_hash}");
    }
    println!("build_date: {}", built_info::BUILT_TIME_UTC);
}
