use std::collections::HashSet;
use std::fmt::Debug;

#[derive(Debug)]
pub struct Command {
    pub urls: HashSet<String>,
}

#[derive(Debug)]
pub struct CommandResult {
    pub url: String,
    pub output: String,
}
