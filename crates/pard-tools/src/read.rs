use serde::Deserialize;

use crate::{Outcome, Workspace, cut_head};

#[derive(Deserialize)]
pub struct Input {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

pub fn run(ws: &Workspace, input: &Input) -> Outcome {
    let path = ws.path(&input.path);
    let text = match std::fs::read(&path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => return Outcome::fail(format!("{} is not a text file", input.path)),
        },
        Err(e) if path.is_dir() => {
            return Outcome::fail(format!("{} is a folder ({e}); use glob", input.path));
        }
        Err(e) => return Outcome::fail(format!("cannot read {}: {e}", input.path)),
    };
    let first = input.offset.unwrap_or(1).max(1);
    let lines = text.split_inclusive('\n').skip(first - 1);
    let chosen: String = match input.limit {
        Some(limit) => lines.take(limit).collect(),
        None => lines.collect(),
    };
    if chosen.is_empty() && !text.is_empty() {
        let total = text.lines().count();
        return Outcome::ok(format!(
            "{} has {total} lines; offset {first} is past the end",
            input.path
        ));
    }
    Outcome::ok(cut_head(&chosen, |shown| {
        format!("read with offset {} for the rest", first + shown)
    }))
}
