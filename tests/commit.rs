#![allow(dead_code)]

use release_note::git::Commit;
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
    footer: Option<String>,
    author: Option<String>,
}

impl CommitBuilder {
    pub fn new(first_line: &str) -> Self {
        Self {
            hash: None,
            first_line: first_line.to_string(),
            body: None,
            footer: None,
            author: None,
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

    pub fn with_footer(mut self, footer: &str) -> Self {
        self.footer = Some(footer.to_string());
        self
    }

    pub fn with_author(mut self, author: &str) -> Self {
        self.author = Some(author.to_string());
        self
    }

    pub fn build(self) -> Commit {
        let hash = self.hash.unwrap_or_else(|| generate_hash(&self.first_line));
        Commit {
            hash,
            first_line: self.first_line,
            body: self.body,
            footer: self.footer,
            author: self.author.unwrap_or("will@globe-theatre.com".to_string()),
        }
    }
}
