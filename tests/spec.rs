//! The examples of the Conventional Commits 1.0.0 specification, compiled as written.

use ccc::sema::Bump;
use ccc::{Compilation, Options, compile};

fn accepted(src: &str) -> Compilation {
    let compilation = compile(src, &Options::default());
    assert!(compilation.is_ok(), "{:#?}", compilation.diagnostics);
    compilation
}

#[test]
fn description_and_breaking_change_footer() {
    let c = accepted(
        "feat: allow provided config object to extend other configs\n\n\
         BREAKING CHANGE: `extends` key in config file is now used for extending other config files",
    );
    assert_eq!(c.bump, Bump::Major);
    let commit = c.commit.unwrap();
    assert!(commit.body.is_none());
    assert_eq!(commit.breaking_changes().count(), 1);
}

#[test]
fn bang_to_draw_attention_to_breaking_change() {
    let c = accepted("feat!: send an email to the customer when a product is shipped");
    assert!(c.breaking);
    assert_eq!(c.bump, Bump::Major);
}

#[test]
fn scope_and_bang() {
    let c = accepted("feat(api)!: send an email to the customer when a product is shipped");
    assert_eq!(c.commit.unwrap().header.scope.unwrap().name, "api");
    assert_eq!(c.bump, Bump::Major);
}

#[test]
fn bang_and_breaking_change_footer() {
    let c = accepted(
        "chore!: drop support for Node 6\n\nBREAKING CHANGE: use JavaScript features not available in Node 6.",
    );
    assert_eq!(c.bump, Bump::Major);
    assert_eq!(
        c.commit.unwrap().footers[0].value.text,
        "use JavaScript features not available in Node 6."
    );
}

#[test]
fn no_body() {
    let c = accepted("docs: correct spelling of CHANGELOG");
    assert_eq!(c.bump, Bump::None);
}

#[test]
fn scope_and_no_body() {
    let c = accepted("feat(lang): add Polish language");
    assert_eq!(c.bump, Bump::Minor);
}

#[test]
fn multi_paragraph_body_and_multiple_footers() {
    let c = accepted(
        "fix: prevent racing of requests\n\n\
         Introduce a request id and a reference to latest request. Dismiss\n\
         incoming responses other than from latest request.\n\n\
         Remove timeouts which were used to mitigate the racing issue but are\n\
         obsolete now.\n\n\
         Reviewed-by: Z\n\
         Refs: #123",
    );
    assert_eq!(c.bump, Bump::Patch);
    let commit = c.commit.unwrap();
    assert_eq!(commit.body.unwrap().lines.len(), 5);
    let tokens: Vec<&str> = commit
        .footers
        .iter()
        .map(|f| f.token.name.as_str())
        .collect();
    assert_eq!(tokens, ["Reviewed-by", "Refs"]);
}

#[test]
fn git_comment_lines_are_ignored() {
    let c = accepted(
        "feat: x\n# Please enter the commit message for your changes.\n#\n# On branch main\n",
    );
    assert!(c.commit.unwrap().body.is_none());
}

#[test]
fn diagnostics_come_out_in_source_order() {
    let c = compile("Feat: Add thing.\n\nRefs:", &Options::default());
    let codes: Vec<_> = c.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["E101", "W101", "E106", "E109"]);
    assert_eq!(c.errors(), 3);
    assert_eq!(c.warnings(), 1);
}
