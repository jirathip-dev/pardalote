use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

use serde::Deserialize;

use crate::{Outcome, Workspace, cut_head, files};

#[derive(Deserialize)]
pub struct Input {
    pattern: String,
    path: Option<String>,
}

pub fn run(ws: &Workspace, input: &Input) -> Outcome {
    let root = ws.path(input.path.as_deref().unwrap_or("."));
    let paths = match files(&root, Some(&input.pattern)) {
        Ok(paths) => paths,
        Err(e) => return Outcome::fail(e),
    };
    let mut found: Vec<(SystemTime, String)> = paths
        .into_iter()
        .filter(|p| !binary(p))
        .map(|p| {
            let at = p.metadata().and_then(|m| m.modified());
            (at.unwrap_or(SystemTime::UNIX_EPOCH), ws.show(&p))
        })
        .collect();
    if found.is_empty() {
        return Outcome::ok("no files match".to_owned());
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let list: String = found.into_iter().map(|(_, p)| p + "\n").collect();
    Outcome::ok(cut_head(&list, |_| {
        "more paths are not shown; narrow the pattern or path".to_owned()
    }))
}

fn binary(path: &Path) -> bool {
    let mut head = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(8192).read_to_end(&mut head))
        .map_or(true, |_| head.contains(&0))
}
