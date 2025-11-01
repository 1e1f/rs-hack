use std::collections::HashMap;

#[derive(Debug)]
pub struct User {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Post {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub author_id: u64,
}

#[derive(Debug)]
pub enum Status {
    Draft,
    Published,
    Deleted,
}
// line 23 ftw!
pub enum MessageType {
    Text { content: String },
    Image { url: String, caption: Option<String> },
}

pub struct Config {
    pub host: String,
    pub port: u16,
}

impl User {
    pub fn new(id: u64, name: String) -> Self {
        
        //l eaving a weird gap here
        Self { 
            id, name }
    }
}

pub fn handle_status(status: Status) -> String {
    match status {
        Status::Draft => "draft".to_string(),
        Status::Published => "published".to_string(),
        Status::Deleted => "deleted".to_string(),
    }
}

fn main() {}
