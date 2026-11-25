use std::io::Read;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;

use crate::{MAX_BYTES, Outcome, Workspace, cut_tail};

const DEFAULT_SECS: u64 = 120;
const MAX_SECS: u64 = 3600;

#[derive(Deserialize)]
pub struct Input {
    command: String,
    timeout: Option<u64>,
}

struct Group(Option<u32>);

impl Drop for Group {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            drop(
                std::process::Command::new("kill")
                    .args(["-KILL", "--", &format!("-{pid}")])
                    .stderr(Stdio::null())
                    .status(),
            );
        }
    }
}

pub async fn run(ws: &Workspace, input: Input) -> Outcome {
    let secs = input.timeout.unwrap_or(DEFAULT_SECS).min(MAX_SECS);
    let (reader, writer) = match std::io::pipe() {
        Ok(pipe) => pipe,
        Err(e) => return Outcome::fail(format!("cannot make a pipe: {e}")),
    };
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(&input.command)
        .current_dir(&ws.root)
        .process_group(0)
        .stdin(Stdio::null());
    for var in &ws.secret_vars {
        cmd.env_remove(var);
    }
    let spawned = writer
        .try_clone()
        .and_then(|out| cmd.stdout(out).stderr(writer).spawn());
    drop(cmd);
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => return Outcome::fail(format!("cannot start sh: {e}")),
    };
    let group = Group(child.id());
    let output = tokio::task::spawn_blocking(move || tail(reader));
    let status = tokio::time::timeout(Duration::from_secs(secs), child.wait()).await;
    drop(group);
    let (dropped, text) = output.await.unwrap_or_default();
    let text = cut_tail(&text, dropped, "send the output to a file and read it");
    match status {
        Ok(Ok(status)) => match status.code() {
            Some(code) => Outcome {
                text: format!("exit {code}\n{text}"),
                failed: code != 0,
                diff: None,
            },
            None => Outcome::fail(format!("killed by a signal\n{text}")),
        },
        Ok(Err(e)) => Outcome::fail(format!("cannot wait for sh: {e}\n{text}")),
        Err(_) => Outcome::fail(format!("timed out after {secs} s\n{text}")),
    }
}

fn tail(mut reader: std::io::PipeReader) -> (usize, String) {
    let mut kept = Vec::new();
    let mut dropped = 0;
    let mut buf = [0; 8192];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        kept.extend_from_slice(buf.get(..n).unwrap_or_default());
        if kept.len() > 4 * MAX_BYTES {
            let end = kept.len() - 2 * MAX_BYTES;
            dropped += kept.drain(..end).filter(|b| *b == b'\n').count();
        }
    }
    (dropped, String::from_utf8_lossy(&kept).into_owned())
}
