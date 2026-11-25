use serde::Deserialize;

use crate::{Diff, Outcome, Workspace};

#[derive(Deserialize)]
pub struct Input {
    path: String,
    content: String,
}

pub fn run(ws: &Workspace, input: Input) -> Outcome {
    let path = ws.path(&input.path);
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, &input.content));
    if let Err(e) = written {
        return Outcome::fail(format!("cannot write {}: {e}", input.path));
    }
    Outcome {
        text: format!("wrote {} bytes to {}", input.content.len(), input.path),
        failed: false,
        diff: Some(Diff {
            path,
            old,
            new: input.content,
        }),
    }
}
