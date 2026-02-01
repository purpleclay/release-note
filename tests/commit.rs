#![allow(dead_code)]

use release_note::contributor::Contributor;
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

const BASE_TIMESTAMP: i64 = 1564567890;

pub struct CommitBuilder {
    hash: Option<String>,
    first_line: String,
    body: Option<String>,
    trailers: Vec<GitTrailer>,
    author: Option<String>,
    email: Option<String>,
    contributors: Vec<Contributor>,
    timestamp: Option<i64>,
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
            timestamp: None,
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

    pub fn with_contributor(mut self, username: &str) -> Self {
        self.contributors.push(Contributor {
            username: username.to_string(),
            avatar_url: format!("https://github.com/{}.png", username),
            is_bot: false,
            is_ai: false,
        });
        self
    }

    pub fn with_contributor_bot(mut self, username: &str) -> Self {
        self.contributors.push(Contributor {
            username: username.to_string(),
            avatar_url: format!("https://github.com/{}.png", username),
            is_bot: true,
            is_ai: false,
        });
        self
    }

    pub fn with_contributors(mut self, usernames: Vec<&str>) -> Self {
        self.contributors = usernames
            .iter()
            .map(|s| Contributor {
                username: s.to_string(),
                avatar_url: "https://avatars.githubusercontent.com/u/2651292?v=4".to_string(),
                is_bot: false,
                is_ai: false,
            })
            .collect();
        self
    }

    pub fn with_timestamp(mut self, timestamp: i64) -> Self {
        self.timestamp = Some(timestamp);
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
            timestamp: self.timestamp.unwrap_or(BASE_TIMESTAMP),
        }
    }
}
