#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails loudly on a broken setup"
)]

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use pard_tools::{Outcome, TOOLS, Workspace, run};

fn workspace(name: &str) -> Workspace {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    Workspace {
        root,
        secret_vars: Vec::new(),
    }
}

async fn call(ws: &Workspace, tool: &str, input: serde_json::Value) -> Outcome {
    run(ws, tool, &input.to_string()).await
}

fn numbered(n: usize) -> String {
    (1..=n)
        .map(|i| format!("line {i}\n"))
        .collect::<Vec<_>>()
        .concat()
}

#[test]
fn six_tools_in_fixed_order_with_json_schemas() {
    let names: Vec<_> = TOOLS.iter().map(|t| t.name).collect();
    assert_eq!(names, ["read", "edit", "write", "shell", "grep", "glob"]);
    for tool in &TOOLS {
        let schema: serde_json::Value = serde_json::from_str(tool.schema).unwrap();
        assert_eq!(schema["type"], "object", "{}", tool.name);
    }
}

#[tokio::test]
async fn unknown_tool_and_bad_input_are_failed_results() {
    let ws = workspace("unknown");
    let out = run(&ws, "browse", "{}").await;
    assert!(out.failed);
    assert!(out.text.contains("unknown tool browse"));
    let out = run(&ws, "read", r#"{"path": 3}"#).await;
    assert!(out.failed);
    assert!(out.text.starts_with("bad input for read"));
}

#[tokio::test]
async fn read_prints_text_without_line_numbers_and_takes_offset_and_limit() {
    let ws = workspace("read");
    fs::write(ws.root.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    let out = call(&ws, "read", serde_json::json!({"path": "a.txt"})).await;
    assert!(!out.failed);
    assert_eq!(out.text, "one\ntwo\nthree\n");
    let out = call(
        &ws,
        "read",
        serde_json::json!({"path": "a.txt", "offset": 2, "limit": 1}),
    )
    .await;
    assert_eq!(out.text, "two\n");
    let out = call(&ws, "read", serde_json::json!({"path": "missing.txt"})).await;
    assert!(out.failed);
    assert!(out.text.starts_with("cannot read missing.txt"));
}

#[tokio::test]
async fn read_keeps_the_head_and_says_where_the_rest_starts() {
    let ws = workspace("read-cap");
    fs::write(ws.root.join("long.txt"), numbered(2500)).unwrap();
    let out = call(&ws, "read", serde_json::json!({"path": "long.txt"})).await;
    assert!(out.text.starts_with("line 1\n"));
    assert!(out.text.contains("line 2000\n"));
    assert!(!out.text.contains("line 2001\n"));
    assert!(
        out.text
            .ends_with("[cut after 2000 lines: read with offset 2001 for the rest]")
    );
    let wide = "x".repeat(100) + "\n";
    fs::write(ws.root.join("wide.txt"), wide.repeat(1000)).unwrap();
    let out = call(
        &ws,
        "read",
        serde_json::json!({"path": "wide.txt", "offset": 11}),
    )
    .await;
    assert!(out.text.len() < 48 * 1024 + 100);
    assert!(
        out.text
            .ends_with("[cut after 486 lines: read with offset 497 for the rest]")
    );
}

#[tokio::test]
async fn edit_replaces_one_exact_match() {
    let ws = workspace("edit");
    let file = ws.root.join("a.rs");
    fs::write(&file, "let a = 1;\nlet b = 2;\n").unwrap();
    let out = call(
        &ws,
        "edit",
        serde_json::json!({"path": "a.rs", "old_string": "b = 2", "new_string": "b = 3"}),
    )
    .await;
    assert!(!out.failed, "{}", out.text);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "let a = 1;\nlet b = 3;\n"
    );
    let diff = out.diff.unwrap();
    assert_eq!(diff.old, "let a = 1;\nlet b = 2;\n");
    assert_eq!(diff.new, "let a = 1;\nlet b = 3;\n");
}

#[tokio::test]
async fn edit_with_many_matches_needs_replace_all() {
    let ws = workspace("edit-many");
    let file = ws.root.join("a.txt");
    fs::write(&file, "x\nx\n").unwrap();
    let input = serde_json::json!({"path": "a.txt", "old_string": "x", "new_string": "y"});
    let out = call(&ws, "edit", input).await;
    assert!(out.failed);
    assert_eq!(
        out.text,
        "a.txt: 2 matches: add lines around it or set replace_all"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "x\nx\n");
    let input = serde_json::json!({"path": "a.txt", "old_string": "x", "new_string": "y", "replace_all": true});
    assert!(!call(&ws, "edit", input).await.failed);
    assert_eq!(fs::read_to_string(&file).unwrap(), "y\ny\n");
}

#[tokio::test]
async fn edit_tolerant_pass_reads_trailing_space_and_typography_as_ascii() {
    let ws = workspace("edit-loose");
    let file = ws.root.join("a.txt");
    fs::write(
        &file,
        "say(\u{201C}hi\u{201D});   \nnext \u{2014} line\nend\n",
    )
    .unwrap();
    let input = serde_json::json!({
        "path": "a.txt",
        "old_string": "say(\"hi\");\nnext - line",
        "new_string": "say(\"bye\");"
    });
    let out = call(&ws, "edit", input).await;
    assert!(!out.failed, "{}", out.text);
    assert_eq!(fs::read_to_string(&file).unwrap(), "say(\"bye\");\nend\n");
}

#[tokio::test]
async fn edit_tolerant_pass_must_find_exactly_one_place() {
    let ws = workspace("edit-loose-many");
    let file = ws.root.join("a.txt");
    fs::write(&file, "a \u{2013} b\na \u{2014} b\n").unwrap();
    let input = serde_json::json!({"path": "a.txt", "old_string": "a - b", "new_string": "c", "replace_all": true});
    let out = call(&ws, "edit", input).await;
    assert!(out.failed);
    assert_eq!(
        out.text,
        "a.txt: 2 matches: add lines around it or set replace_all"
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "a \u{2013} b\na \u{2014} b\n"
    );
}

#[tokio::test]
async fn edit_miss_names_the_cause() {
    let ws = workspace("edit-miss");
    fs::write(
        ws.root.join("a.rs"),
        "fn a() {\n    if x {\n        y();\n    }\n}\n",
    )
    .unwrap();
    let input =
        serde_json::json!({"path": "a.rs", "old_string": "if x {\n    y();\n}", "new_string": ""});
    let out = call(&ws, "edit", input).await;
    assert!(out.failed);
    assert_eq!(
        out.text,
        "a.rs: no match, found with different indentation near line 2"
    );
    let input = serde_json::json!({"path": "a.rs", "old_string": "z();", "new_string": ""});
    let out = call(&ws, "edit", input).await;
    assert_eq!(out.text, "a.rs: no match");
}

#[tokio::test]
async fn write_makes_parent_folders() {
    let ws = workspace("write");
    let out = call(
        &ws,
        "write",
        serde_json::json!({"path": "deep/er/a.txt", "content": "hi\n"}),
    )
    .await;
    assert!(!out.failed, "{}", out.text);
    assert_eq!(out.text, "wrote 3 bytes to deep/er/a.txt");
    assert_eq!(
        fs::read_to_string(ws.root.join("deep/er/a.txt")).unwrap(),
        "hi\n"
    );
    assert_eq!(out.diff.unwrap().old, "");
}

#[tokio::test]
async fn shell_gives_exit_code_then_stdout_and_stderr_together() {
    let ws = workspace("shell");
    let out = call(
        &ws,
        "shell",
        serde_json::json!({"command": "echo out; echo err >&2; pwd"}),
    )
    .await;
    assert!(!out.failed);
    let pwd = ws.root.canonicalize().unwrap();
    assert_eq!(out.text, format!("exit 0\nout\nerr\n{}\n", pwd.display()));
    let out = call(&ws, "shell", serde_json::json!({"command": "exit 3"})).await;
    assert!(out.failed);
    assert_eq!(out.text, "exit 3\n");
}

#[tokio::test]
async fn shell_runs_without_the_provider_key_variables() {
    let mut ws = workspace("shell-env");
    let probe = serde_json::json!({"command": "echo \"${CARGO_MANIFEST_DIR-unset}\""});
    let out = call(&ws, "shell", probe.clone()).await;
    assert_ne!(out.text, "exit 0\nunset\n");
    ws.secret_vars = vec!["CARGO_MANIFEST_DIR".to_owned()];
    let out = call(&ws, "shell", probe).await;
    assert_eq!(out.text, "exit 0\nunset\n");
}

#[tokio::test]
async fn shell_keeps_the_tail_of_long_output() {
    let ws = workspace("shell-cap");
    let out = call(&ws, "shell", serde_json::json!({"command": "seq 1 5000"})).await;
    assert!(out.text.starts_with(
        "exit 0\n[cut: the first 3000 lines are not shown; send the output to a file and read it]\n3001\n"
    ));
    assert!(out.text.ends_with("\n5000\n"));
    let out = call(&ws, "shell", serde_json::json!({"command": "seq 1 300000"})).await;
    assert!(out.text.len() < 48 * 1024 + 200);
    assert!(out.text.ends_with("\n300000\n"));
    let cut: usize = out.text.split(' ').nth(4).unwrap().parse().unwrap();
    let shown = out.text.lines().count() - 2;
    assert_eq!(cut + shown, 300_000);
}

#[tokio::test]
async fn shell_kills_what_the_command_left_running() {
    let ws = workspace("shell-bg");
    let started = tokio::time::timeout(
        Duration::from_secs(20),
        call(
            &ws,
            "shell",
            serde_json::json!({"command": "sleep 60 & echo started"}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(started.text, "exit 0\nstarted\n");
    let out = call(
        &ws,
        "shell",
        serde_json::json!({"command": "echo before; sleep 60", "timeout": 1}),
    )
    .await;
    assert!(out.failed);
    assert_eq!(out.text, "timed out after 1 s\nbefore\n");
}

#[tokio::test]
async fn grep_lists_path_line_text_in_path_order_skipping_ignored_and_binary() {
    let ws = workspace("grep");
    fs::create_dir_all(ws.root.join("src")).unwrap();
    fs::write(ws.root.join("src/b.rs"), "fn b() {}\n").unwrap();
    fs::write(ws.root.join("src/a.rs"), "// x\nfn a() {}\n").unwrap();
    fs::write(ws.root.join("notes.md"), "fn in notes\n").unwrap();
    fs::write(ws.root.join("blob.bin"), b"fn \0 binary").unwrap();
    fs::write(ws.root.join(".gitignore"), "build/\n").unwrap();
    fs::create_dir_all(ws.root.join("build")).unwrap();
    fs::write(ws.root.join("build/out.rs"), "fn built() {}\n").unwrap();
    let out = call(&ws, "grep", serde_json::json!({"pattern": "^fn"})).await;
    assert_eq!(
        out.text,
        "notes.md:1:fn in notes\nsrc/a.rs:2:fn a() {}\nsrc/b.rs:1:fn b() {}\n"
    );
    let out = call(
        &ws,
        "grep",
        serde_json::json!({"pattern": "fn", "glob": "*.rs"}),
    )
    .await;
    assert_eq!(out.text, "src/a.rs:2:fn a() {}\nsrc/b.rs:1:fn b() {}\n");
    let out = call(
        &ws,
        "grep",
        serde_json::json!({"pattern": "fn", "path": "src"}),
    )
    .await;
    assert_eq!(out.text, "src/a.rs:2:fn a() {}\nsrc/b.rs:1:fn b() {}\n");
    let out = call(&ws, "grep", serde_json::json!({"pattern": "("})).await;
    assert!(out.failed);
    assert!(out.text.starts_with("bad regex"));
}

#[tokio::test]
async fn grep_keeps_the_head_of_many_matches() {
    let ws = workspace("grep-cap");
    fs::write(ws.root.join("a.txt"), numbered(3000)).unwrap();
    let out = call(&ws, "grep", serde_json::json!({"pattern": "line"})).await;
    assert!(out.text.starts_with("a.txt:1:line 1\n"));
    assert!(out.text.contains("a.txt:2000:line 2000\n"));
    assert!(!out.text.contains("a.txt:2001:"));
    assert!(out.text.ends_with(
        "[cut after 2000 lines: more matches are not shown; narrow the pattern, path or glob]"
    ));
}

#[tokio::test]
async fn glob_lists_paths_newest_first() {
    let ws = workspace("glob");
    fs::create_dir_all(ws.root.join("src")).unwrap();
    for (name, age) in [
        ("src/old.rs", 300),
        ("src/new.rs", 100),
        ("mid.rs", 200),
        ("x.txt", 0),
    ] {
        let path = ws.root.join(name);
        fs::write(&path, "text\n").unwrap();
        let at = SystemTime::now() - Duration::from_secs(age);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }
    fs::write(ws.root.join("blob.rs"), b"\0\0").unwrap();
    let out = call(&ws, "glob", serde_json::json!({"pattern": "*.rs"})).await;
    assert_eq!(out.text, "src/new.rs\nmid.rs\nsrc/old.rs\n");
    let out = call(
        &ws,
        "glob",
        serde_json::json!({"pattern": "*.rs", "path": "src"}),
    )
    .await;
    assert_eq!(out.text, "src/new.rs\nsrc/old.rs\n");
    let out = call(&ws, "glob", serde_json::json!({"pattern": "*.go"})).await;
    assert_eq!(out.text, "no files match");
}
