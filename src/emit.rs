//! What the compiler writes out: JSON, and the message in canonical form.

use serde::Serialize;

use crate::Compilation;
use crate::ast::Commit;
use crate::diagnostic::Diagnostic;
use crate::sema::Bump;

/// The full outcome of a compilation, for tools.
#[derive(Serialize)]
pub struct Report<'a> {
    pub ok: bool,
    pub breaking: bool,
    pub bump: Bump,
    pub commit: Option<&'a Commit>,
    pub diagnostics: &'a [Diagnostic],
}

impl<'a> Report<'a> {
    pub fn new(compilation: &'a Compilation) -> Self {
        Self {
            ok: compilation.is_ok(),
            breaking: compilation.breaking,
            bump: compilation.bump,
            commit: compilation.commit.as_ref(),
            diagnostics: &compilation.diagnostics,
        }
    }
}

pub fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).expect("the syntax tree is serialisable")
}

/// Rewrites the message in canonical form: lowercase type, trimmed parts, `BREAKING CHANGE: `
/// for every breaking footer, one blank line between sections, a final newline.
pub fn canonical(commit: &Commit) -> String {
    let header = &commit.header;
    let mut first = header.kind.name.to_lowercase();
    if let Some(scope) = &header.scope {
        first.push_str(&format!("({})", scope.name.trim()));
    }
    if header.bang.is_some() {
        first.push('!');
    }
    first.push_str(": ");
    first.push_str(&header.description.text);

    let mut sections = vec![first];
    if let Some(body) = &commit.body {
        sections.push(body.text.clone());
    }
    if !commit.footers.is_empty() {
        let footers: Vec<String> = commit
            .footers
            .iter()
            .map(|f| {
                let (token, separator) = if f.token.looks_breaking() {
                    ("BREAKING CHANGE", ": ")
                } else {
                    (f.token.name.as_str(), f.separator.as_str())
                };
                format!("{token}{separator}{}", f.value.text)
            })
            .collect();
        sections.push(footers.join("\n"));
    }
    sections.join("\n\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, compile};

    #[test]
    fn canonical_form_normalises_what_the_spec_allows() {
        let src =
            "Feat( api ): add thing   \n\n\nbody line\n\nbreaking-change #old key\nRefs: #1\n";
        let compilation = compile(src, &Options::default());
        let commit = compilation.commit.expect("parsed");
        assert_eq!(
            canonical(&commit),
            "feat(api): add thing\n\nbody line\n\nBREAKING CHANGE: old key\nRefs: #1\n"
        );
    }
}
