#![allow(dead_code)]

use release_note::git::{Commit, GitTrailer};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn generate_hash(first_line: &str) -> String {
    let mut hasher = DefaultHasher::new();
    first_line.hash(&mut hasher);
    let hash_value = hasher.finish();
    format!(
        "{:016x}{:016x}{:08x}",
        hash_value,
        hash_value,
        (hash_value >> 32) as u32
    )
}

pub struct CommitBuilder {
    hash: Option<String>,
    first_line: String,
    body: Option<String>,
    trailers: Vec<GitTrailer>,
    author: Option<String>,
    email: Option<String>,
    contributors: Vec<String>,
}

impl CommitBuilder {
    pub fn new(first_line: &str) -> Self {
        Self {
            hash: None,
            first_line: first_line.to_string(),
            body: None,
            trailers: Vec::new(),
            author: None,
            email: None,
            contributors: Vec::new(),
        }
    }

    pub fn with_hash(mut self, hash: &str) -> Self {
        self.hash = Some(hash.to_string());
        self
    }

    pub fn with_body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self
    }

    pub fn with_trailer(mut self, key: &str, value: &str) -> Self {
        self.trailers.push(GitTrailer::from_key_value(
            key.to_string(),
            value.to_string(),
        ));
        self
    }

    pub fn with_author(mut self, author: &str) -> Self {
        self.author = Some(author.to_string());
        self
    }

    pub fn with_email(mut self, email: &str) -> Self {
        self.email = Some(email.to_string());
        self
    }

    pub fn with_contributor(mut self, contributor: &str) -> Self {
        self.contributors.push(contributor.to_string());
        self
    }

    pub fn with_contributors(mut self, contributors: Vec<&str>) -> Self {
        self.contributors = contributors.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn build(self) -> Commit {
        let hash = self.hash.unwrap_or_else(|| generate_hash(&self.first_line));
        Commit {
            hash,
            first_line: self.first_line,
            body: self.body,
            trailers: self.trailers,
            linked_issues: Vec::new(),
            author: self.author.unwrap_or("William Shakespeare".to_string()),
            email: self.email.unwrap_or("will@globe-theatre.com".to_string()),
            contributors: self.contributors,
        }
    }
}
