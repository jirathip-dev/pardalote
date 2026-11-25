use regex::Regex;
use serde::Deserialize;

use crate::{MAX_BYTES, MAX_LINES, Outcome, Workspace, cut_head, files};

#[derive(Deserialize)]
pub struct Input {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
}

pub fn run(ws: &Workspace, input: &Input) -> Outcome {
    let regex = match Regex::new(&input.pattern) {
        Ok(regex) => regex,
        Err(e) => return Outcome::fail(format!("bad regex: {e}")),
    };
    let root = ws.path(input.path.as_deref().unwrap_or("."));
    let paths = match files(&root, input.glob.as_deref()) {
        Ok(paths) => paths,
        Err(e) => return Outcome::fail(e),
    };
    let mut found = Vec::new();
    let mut bytes = 0;
    'files: for path in paths {
        let Some(text) = std::fs::read(&path)
            .ok()
            .filter(|b| !b.contains(&0))
            .and_then(|b| String::from_utf8(b).ok())
        else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                let hit = format!("{}:{}:{line}\n", ws.show(&path), n + 1);
                bytes += hit.len();
                found.push(hit);
                if found.len() > MAX_LINES || bytes > MAX_BYTES {
                    break 'files;
                }
            }
        }
    }
    if found.is_empty() {
        return Outcome::ok("no matches".to_owned());
    }
    let out = found.concat();
    Outcome::ok(cut_head(&out, |_| {
        "more matches are not shown; narrow the pattern, path or glob".to_owned()
    }))
}
