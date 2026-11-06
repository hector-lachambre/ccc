use std::io::Write;
use std::process::{Command, Output, Stdio};

fn ccc(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ccc"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("ccc starts");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn accepts_a_valid_message_silently() {
    let out = ccc(&[], "feat(cli): read the message from stdin\n");
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());
}

#[test]
fn rejects_with_a_rendered_diagnostic() {
    let out = ccc(&["--color", "never"], "feat add thing\n");
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("error[E005]: expected `:` after the type"),
        "{stderr}"
    );
    assert!(stderr.contains(" --> <stdin>:1:5"), "{stderr}");
    assert!(
        stderr.ends_with("error: the message was rejected due to 1 error\n"),
        "{stderr}"
    );
}

#[test]
fn emits_the_bump_and_the_canonical_form() {
    let out = ccc(
        &[
            "-m",
            "Fix(Parser): Handle CRLF",
            "--emit",
            "canonical",
            "--allow-capitalized",
            "-q",
        ],
        "",
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert!(out.stdout.is_empty());

    let out = ccc(
        &[
            "-m",
            "fix(parser): handle CRLF\n\nBREAKING-CHANGE #none",
            "--emit",
            "canonical",
            "-q",
        ],
        "",
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        text(&out.stdout),
        "fix(parser): handle CRLF\n\nBREAKING CHANGE: none\n"
    );

    let out = ccc(&["-m", "fix: x", "--emit", "bump"], "");
    assert_eq!(text(&out.stdout), "patch\n");
}

#[test]
fn warnings_can_be_denied() {
    let out = ccc(&["-q"], "feat: Add thing\n");
    assert_eq!(out.status.code(), Some(0));
    let out = ccc(&["-q", "-D"], "feat: Add thing\n");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn rules_come_from_flags() {
    let out = ccc(
        &[
            "-q",
            "--types",
            "any",
            "--scopes",
            "api,ui",
            "--require-scope",
        ],
        "release: x\n",
    );
    assert_eq!(out.status.code(), Some(1));
    let out = ccc(
        &[
            "-q",
            "--types",
            "any",
            "--scopes",
            "api,ui",
            "--require-scope",
        ],
        "release(ui): x\n",
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn report_is_json_even_when_rejected() {
    let out = ccc(&["--emit", "report", "-q"], "nope\n");
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(report["commit"], serde_json::Value::Null);
    assert_eq!(report["diagnostics"][0]["code"], "E005");
}

#[test]
fn missing_file_is_a_usage_error() {
    let out = ccc(&["/nonexistent/COMMIT_EDITMSG"], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).starts_with("error: /nonexistent/COMMIT_EDITMSG"));
}
