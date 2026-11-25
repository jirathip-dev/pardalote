use serde::Deserialize;

use crate::{Diff, Outcome, Workspace};

#[derive(Deserialize)]
pub struct Input {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

pub fn run(ws: &Workspace, input: &Input) -> Outcome {
    let path = ws.path(&input.path);
    let old = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => return Outcome::fail(format!("cannot read {}: {e}", input.path)),
    };
    if input.old_string.is_empty() {
        return Outcome::fail("old_string is empty; use write to make a file".to_owned());
    }
    let new = match replace(&old, input) {
        Ok(new) => new,
        Err(why) => return Outcome::fail(format!("{}: {why}", input.path)),
    };
    if let Err(e) = std::fs::write(&path, &new) {
        return Outcome::fail(format!("cannot write {}: {e}", input.path));
    }
    Outcome {
        text: format!("edited {}", input.path),
        failed: false,
        diff: Some(Diff { path, old, new }),
    }
}

fn replace(text: &str, input: &Input) -> Result<String, String> {
    let exact = text.matches(&input.old_string).count();
    if exact == 1 || (exact > 1 && input.replace_all) {
        return Ok(text.replace(&input.old_string, &input.new_string));
    }
    if exact > 1 {
        return Err(format!(
            "{exact} matches: add lines around it or set replace_all"
        ));
    }
    let loose = Folded::new(text, false);
    let wanted = Folded::new(&input.old_string, false).text;
    let found: Vec<usize> = if wanted.is_empty() {
        Vec::new()
    } else {
        loose
            .text
            .match_indices(&wanted)
            .map(|(at, _)| at)
            .collect()
    };
    match found.as_slice() {
        [at] => {
            let (start, end) = loose.span(*at, wanted.len()).ok_or("no match")?;
            let mut out = String::with_capacity(text.len() + input.new_string.len());
            out.push_str(text.get(..start).unwrap_or_default());
            out.push_str(&input.new_string);
            out.push_str(text.get(end..).unwrap_or_default());
            Ok(out)
        }
        [] => Err(indent_hint(text, &input.old_string)),
        many => Err(format!(
            "{} matches: add lines around it or set replace_all",
            many.len()
        )),
    }
}

fn indent_hint(text: &str, old_string: &str) -> String {
    let flat = Folded::new(text, true);
    let wanted = Folded::new(old_string, true).text;
    let at = (!wanted.is_empty())
        .then(|| flat.text.find(&wanted))
        .flatten()
        .and_then(|at| flat.span(at, wanted.len()));
    match at {
        Some((start, _)) => {
            let line = text.get(..start).unwrap_or_default().matches('\n').count() + 1;
            format!("no match, found with different indentation near line {line}")
        }
        None => "no match".to_owned(),
    }
}

struct Folded {
    text: String,
    starts: Vec<usize>,
    ends: Vec<usize>,
}

impl Folded {
    fn new(text: &str, flat_indent: bool) -> Self {
        let mut folded = Self {
            text: String::with_capacity(text.len()),
            starts: Vec::with_capacity(text.len()),
            ends: Vec::with_capacity(text.len()),
        };
        let mut at = 0;
        for line in text.split_inclusive('\n') {
            let body = line.strip_suffix('\n').unwrap_or(line);
            let kept = body.trim_end();
            let lead = if flat_indent {
                kept.len() - kept.trim_start().len()
            } else {
                0
            };
            for (i, c) in kept.char_indices().skip_while(|(i, _)| *i < lead) {
                folded.push(ascii(c), at + i, at + i + c.len_utf8());
            }
            if body.len() < line.len() {
                folded.push('\n', at + body.len(), at + line.len());
            }
            at += line.len();
        }
        folded
    }

    fn push(&mut self, c: char, start: usize, end: usize) {
        self.text.push(c);
        for _ in 0..c.len_utf8() {
            self.starts.push(start);
            self.ends.push(end);
        }
    }

    fn span(&self, at: usize, len: usize) -> Option<(usize, usize)> {
        Some((*self.starts.get(at)?, *self.ends.get(at + len - 1)?))
    }
}

fn ascii(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
        '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
        '\u{00A0}' | '\u{2002}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => ' ',
        c => c,
    }
}
