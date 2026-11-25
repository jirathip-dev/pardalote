//! pard-tools: the six tools pard offers the model.

mod edit;
mod glob;
mod grep;
mod read;
mod shell;
mod write;

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

const MAX_LINES: usize = 2000;
const MAX_BYTES: usize = 48 * 1024;

/// A tool as the model sees it: its name, what it does and its JSON input schema.
pub struct Tool {
    /// The name the model calls it by.
    pub name: &'static str,
    /// What it does, for the model.
    pub description: &'static str,
    /// The JSON schema of its input.
    pub schema: &'static str,
}

/// The six tools, in the fixed order every request lists them.
pub const TOOLS: [Tool; 6] = [
    Tool {
        name: "read",
        description: "Read a text file. Prints the text as it is, without line numbers.",
        schema: r#"{"type":"object","properties":{"path":{"type":"string"},"offset":{"type":"integer","description":"first line, from 1"},"limit":{"type":"integer","description":"number of lines"}},"required":["path"]}"#,
    },
    Tool {
        name: "edit",
        description: "Replace old_string with new_string in a file. old_string must match one place, or set replace_all.",
        schema: r#"{"type":"object","properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"},"replace_all":{"type":"boolean"}},"required":["path","old_string","new_string"]}"#,
    },
    Tool {
        name: "write",
        description: "Write a whole file, making its parent folders.",
        schema: r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
    },
    Tool {
        name: "shell",
        description: "Run a command with sh -c in the session's folder. Prints the exit code, then stdout and stderr together.",
        schema: r#"{"type":"object","properties":{"command":{"type":"string"},"timeout":{"type":"integer","description":"seconds, default 120, at most 3600"}},"required":["command"]}"#,
    },
    Tool {
        name: "grep",
        description: "Search file contents with a regex. Prints path:line:text in path order.",
        schema: r#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"glob":{"type":"string","description":"only files matching this glob"}},"required":["pattern"]}"#,
    },
    Tool {
        name: "glob",
        description: "List files matching a glob, newest first.",
        schema: r#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"}},"required":["pattern"]}"#,
    },
];

/// Where tools run: the session's folder and the variables `shell` never passes on.
pub struct Workspace {
    /// The session's folder; relative paths are under it.
    pub root: PathBuf,
    /// Every provider's key variable, removed from `shell`'s environment.
    pub secret_vars: Vec<String>,
}

impl Workspace {
    fn path(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    fn show(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

/// A file a tool changed, with its text before and after.
pub struct Diff {
    /// The file.
    pub path: PathBuf,
    /// Its text before, empty for a new file.
    pub old: String,
    /// Its text after.
    pub new: String,
}

/// What a tool call gives back to the model.
pub struct Outcome {
    /// The text the model reads, already cut to the cap.
    pub text: String,
    /// The call failed; the text says why.
    pub failed: bool,
    /// The file `edit` or `write` changed.
    pub diff: Option<Diff>,
}

impl Outcome {
    fn ok(text: String) -> Self {
        Self {
            text,
            failed: false,
            diff: None,
        }
    }

    fn fail(text: String) -> Self {
        Self {
            text,
            failed: true,
            diff: None,
        }
    }
}

/// Run the tool named `tool` on `input`, the arguments exactly as the model sent them.
pub async fn run(ws: &Workspace, tool: &str, input: &str) -> Outcome {
    match tool {
        "read" => parsed(tool, input, |i| read::run(ws, &i)),
        "edit" => parsed(tool, input, |i| edit::run(ws, &i)),
        "write" => parsed(tool, input, |i| write::run(ws, i)),
        "grep" => parsed(tool, input, |i| grep::run(ws, &i)),
        "glob" => parsed(tool, input, |i| glob::run(ws, &i)),
        "shell" => match serde_json::from_str(input) {
            Ok(i) => shell::run(ws, i).await,
            Err(e) => bad_input(tool, &e),
        },
        _ => Outcome::fail(format!(
            "unknown tool {tool}; the tools are read, edit, write, shell, grep and glob"
        )),
    }
}

fn parsed<I: DeserializeOwned>(tool: &str, input: &str, f: impl FnOnce(I) -> Outcome) -> Outcome {
    match serde_json::from_str(input) {
        Ok(i) => f(i),
        Err(e) => bad_input(tool, &e),
    }
}

fn bad_input(tool: &str, e: &serde_json::Error) -> Outcome {
    Outcome::fail(format!("bad input for {tool}: {e}"))
}

fn cut_head(text: &str, rest: impl FnOnce(usize) -> String) -> String {
    let mut end = 0;
    for (n, line) in text.split_inclusive('\n').enumerate() {
        if n == MAX_LINES || end + line.len() > MAX_BYTES {
            break;
        }
        end += line.len();
    }
    if end == text.len() {
        return text.to_owned();
    }
    if end == 0 {
        end = text.floor_char_boundary(MAX_BYTES);
    }
    let kept = text.get(..end).unwrap_or_default();
    let shown = kept.lines().count();
    let sep = if kept.ends_with('\n') { "" } else { "\n" };
    format!("{kept}{sep}[cut after {shown} lines: {}]", rest(shown))
}

fn cut_tail(text: &str, dropped: usize, rest: &str) -> String {
    let mut start = text.len();
    for (n, line) in text.split_inclusive('\n').rev().enumerate() {
        if n == MAX_LINES || text.len() - start + line.len() > MAX_BYTES {
            break;
        }
        start -= line.len();
    }
    if start == 0 && dropped == 0 {
        return text.to_owned();
    }
    if start == text.len() {
        start = text.ceil_char_boundary(text.len() - MAX_BYTES);
    }
    let cut = dropped + text.get(..start).unwrap_or_default().lines().count();
    let kept = text.get(start..).unwrap_or_default();
    format!("[cut: the first {cut} lines are not shown; {rest}]\n{kept}")
}

fn files(root: &Path, glob: Option<&str>) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Err(format!("{} does not exist", root.display()));
    }
    let mut walk = ignore::WalkBuilder::new(root);
    walk.require_git(false).sort_by_file_path(Path::cmp);
    if let Some(glob) = glob {
        let overrides = ignore::overrides::OverrideBuilder::new(root)
            .add(glob)
            .and_then(|b| b.build())
            .map_err(|e| format!("bad glob: {e}"))?;
        walk.overrides(overrides);
    }
    Ok(walk
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(ignore::DirEntry::into_path)
        .collect())
}
