use anyhow::Result;
use clap::{Parser, arg};
use std::path::PathBuf;

use crate::git::GitRepo;

mod git;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// A starting reference within the git history. Defaults to HEAD.
    ///
    /// A reference can be:
    ///  - A commit hash (full or abbreviated).
    ///  - A tag (1.0.0 or refs/tags/1.0.0).
    ///  - A branch name (local or remote).
    ///  - Or a relative reference (HEAD, HEAD~3).
    #[arg(value_name = "FROM", required = false, verbatim_doc_comment)]
    from: Option<String>,

    /// Path to the root working directory of a repository
    #[arg(value_name = "DIR", long, default_value = ".")]
    path: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let repo = GitRepo::open(args.path)?;

    let history = repo.history(args.from)?;
    for commit in history {
        println!("- {} {}", commit.hash, commit.message);
    }
    Ok(())
}
