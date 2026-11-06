//! Semantic analysis: the checks the grammar alone cannot express, and what the message means
//! for the version number.

use std::fmt;

use serde::Serialize;

use crate::ast::{Body, Commit, Footer, Header, Ident, Separator, Text};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// The types of the Angular convention, the default allow-list.
pub const DEFAULT_TYPES: &[&str] = &[
    "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert",
];

#[derive(Debug, Clone)]
pub struct Rules {
    /// Accepted types, compared case-insensitively. `None` accepts any word.
    pub types: Option<Vec<String>>,
    /// Accepted scopes. `None` accepts any.
    pub scopes: Option<Vec<String>>,
    pub require_scope: bool,
    /// Longest accepted header, in characters.
    pub max_header_length: Option<usize>,
    /// Longest accepted body line, in characters.
    pub max_body_line_length: Option<usize>,
    /// Warn when the description starts with a capitalised word.
    pub lowercase_description: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            types: Some(DEFAULT_TYPES.iter().map(|t| t.to_string()).collect()),
            scopes: None,
            require_scope: false,
            max_header_length: Some(100),
            max_body_line_length: Some(100),
            lowercase_description: true,
        }
    }
}

/// The semver bump a message calls for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Bump {
    None,
    Patch,
    Minor,
    Major,
}

impl Bump {
    pub const fn as_str(self) -> &'static str {
        match self {
            Bump::None => "none",
            Bump::Patch => "patch",
            Bump::Minor => "minor",
            Bump::Major => "major",
        }
    }
}

impl fmt::Display for Bump {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct Analysis {
    pub breaking: bool,
    pub bump: Bump,
    pub diagnostics: Vec<Diagnostic>,
}

/// Checks `commit` against the specification and `rules`. `src` is the text it was parsed from.
pub fn analyze(commit: &Commit, src: &str, rules: &Rules) -> Analysis {
    let mut sema = Sema {
        src,
        rules,
        diagnostics: Vec::new(),
    };
    sema.header(&commit.header);
    if let Some(body) = &commit.body {
        sema.body(body);
    }
    for footer in &commit.footers {
        sema.footer(footer);
    }
    let breaking = commit.is_breaking();
    let bump = match (breaking, commit.header.kind.name.to_lowercase().as_str()) {
        (true, _) => Bump::Major,
        (false, "feat") => Bump::Minor,
        (false, "fix") => Bump::Patch,
        (false, _) => Bump::None,
    };
    Analysis {
        breaking,
        bump,
        diagnostics: sema.diagnostics,
    }
}

struct Sema<'a> {
    src: &'a str,
    rules: &'a Rules,
    diagnostics: Vec<Diagnostic>,
}

impl Sema<'_> {
    fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    fn header(&mut self, header: &Header) {
        self.kind(&header.kind);
        match &header.scope {
            Some(scope) => self.scope(scope),
            None if self.rules.require_scope => {
                self.report(Diagnostic::error("E103", "a scope is required").label(
                    Span::empty(header.kind.span.end),
                    "add a scope here, e.g. `(api)`",
                ))
            }
            None => {}
        }
        self.description(&header.description);
        if let Some(max) = self.rules.max_header_length {
            self.length(header.span, max, "header", "W102");
        }
    }

    fn kind(&mut self, kind: &Ident) {
        let lower = kind.name.to_lowercase();
        if kind.name != lower {
            self.report(
                Diagnostic::error("E101", "the type must be lowercase")
                    .label(kind.span, format!("`{}` is not lowercase", kind.name))
                    .help(format!("write `{lower}`")),
            );
        }
        let Some(types) = &self.rules.types else {
            return;
        };
        if types.iter().any(|t| t.to_lowercase() == lower) {
            return;
        }
        let diagnostic = Diagnostic::error("E102", format!("unknown type `{}`", kind.name))
            .label(kind.span, "not an allowed type");
        self.report(match closest(&kind.name, types) {
            Some(suggestion) => diagnostic.help(format!("did you mean `{suggestion}`?")),
            None => diagnostic.help(format!("allowed types are {}", list(types))),
        });
    }

    fn scope(&mut self, scope: &Ident) {
        if scope.name.chars().any(char::is_whitespace) {
            let fixed = scope.name.split_whitespace().collect::<Vec<_>>().join("-");
            self.report(
                Diagnostic::error("E105", "the scope must not contain whitespace")
                    .label(scope.span, "scope")
                    .help(format!("write `({fixed})`")),
            );
            return;
        }
        let Some(scopes) = &self.rules.scopes else {
            return;
        };
        if scopes.contains(&scope.name) {
            return;
        }
        let diagnostic = Diagnostic::error("E104", format!("unknown scope `{}`", scope.name))
            .label(scope.span, "not an allowed scope");
        self.report(match closest(&scope.name, scopes) {
            Some(suggestion) => diagnostic.help(format!("did you mean `{suggestion}`?")),
            None => diagnostic.help(format!("allowed scopes are {}", list(scopes))),
        });
    }

    fn description(&mut self, description: &Text) {
        let text = &description.text;
        if text.is_empty() {
            return;
        }
        if text.ends_with('.') {
            let period = Span::new(description.span.end - 1, description.span.end);
            self.report(
                Diagnostic::error("E106", "the description must not end with a period")
                    .label(period, "remove this period"),
            );
        }
        let mut chars = text.chars();
        if self.rules.lowercase_description
            && let (Some(first), Some(second)) = (chars.next(), chars.next())
            && first.is_uppercase()
            && second.is_lowercase()
        {
            let word = text.split_whitespace().next().unwrap_or_default();
            let fixed: String = first.to_lowercase().chain(word.chars().skip(1)).collect();
            self.report(
                Diagnostic::warning("W101", "the description should start in lowercase")
                    .label(
                        Span::new(description.span.start, description.span.start + word.len()),
                        "capitalised",
                    )
                    .help(format!("write `{fixed}`")),
            );
        }
    }

    fn body(&mut self, body: &Body) {
        for line in &body.lines {
            if let Some(max) = self.rules.max_body_line_length {
                self.length(line.span, max, "body line", "W103");
            }
            if opens_a_breaking_footer(&line.text) {
                self.report(
                    Diagnostic::error("E107", "`BREAKING CHANGE` must be a footer, not body text")
                        .label(line.span, "read as part of the body")
                        .help(
                            "footers are the last paragraphs of the message; move this line there",
                        ),
                );
            }
        }
    }

    fn footer(&mut self, footer: &Footer) {
        let token = &footer.token;
        if token.looks_breaking() && !token.is_breaking() {
            self.report(
                Diagnostic::error(
                    "E108",
                    "the breaking change token must be written `BREAKING CHANGE`",
                )
                .label(token.span, format!("`{}` is not accepted", token.name))
                .help("write `BREAKING CHANGE: ...` (`BREAKING-CHANGE` is accepted too)"),
            );
        }
        if token.is_breaking() && footer.separator == Separator::Hash {
            self.report(
                Diagnostic::warning("W104", "`BREAKING CHANGE` should be followed by `: `")
                    .label(
                        Span::new(token.span.end, footer.value.span.start),
                        "` #` separator",
                    )
                    .help(format!("write `BREAKING CHANGE: {}`", footer.value.text)),
            );
        }
        if footer.value.text.is_empty() {
            self.report(
                Diagnostic::error("E109", format!("footer `{}` has no value", token.name))
                    .label(footer.value.span, "expected a value here"),
            );
        }
    }

    fn length(&mut self, span: Span, max: usize, what: &str, code: &'static str) {
        let text = span.slice(self.src);
        let len = text.chars().count();
        if len <= max {
            return;
        }
        let overflow = text
            .char_indices()
            .nth(max)
            .map_or(span.end, |(i, _)| span.start + i);
        self.report(
            Diagnostic::warning(
                code,
                format!("the {what} is {len} characters long, the limit is {max}"),
            )
            .label(
                Span::new(overflow, span.end),
                format!("{} characters over the limit", len - max),
            ),
        );
    }
}

fn opens_a_breaking_footer(line: &str) -> bool {
    let lower = line.to_lowercase();
    ["breaking change", "breaking-change"]
        .iter()
        .filter_map(|prefix| lower.strip_prefix(prefix))
        .any(|rest| rest.starts_with(':') || rest.starts_with(" #"))
}

/// The candidate within two edits of `word`, if any.
fn closest<'a>(word: &str, candidates: &'a [String]) -> Option<&'a str> {
    let word = word.to_lowercase();
    candidates
        .iter()
        .map(|c| (levenshtein(&word, &c.to_lowercase()), c.as_str()))
        .filter(|&(distance, _)| distance <= 2)
        .min_by_key(|&(distance, _)| distance)
        .map(|(_, c)| c)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, &cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

fn list(items: &[String]) -> String {
    items
        .iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::parse;

    fn check(src: &str, rules: &Rules) -> Analysis {
        let parsed = parse(src, tokenize(src, true));
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        analyze(&parsed.commit.unwrap(), src, rules)
    }

    fn codes(src: &str) -> Vec<&'static str> {
        check(src, &Rules::default())
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn computes_the_bump() {
        assert_eq!(check("feat: x", &Rules::default()).bump, Bump::Minor);
        assert_eq!(check("fix: x", &Rules::default()).bump, Bump::Patch);
        assert_eq!(check("docs: x", &Rules::default()).bump, Bump::None);
        assert_eq!(check("docs!: x", &Rules::default()).bump, Bump::Major);
        assert_eq!(
            check("fix: x\n\nBREAKING-CHANGE: y", &Rules::default()).bump,
            Bump::Major
        );
    }

    #[test]
    fn checks_the_type() {
        assert_eq!(codes("Feat: x"), ["E101"]);
        assert_eq!(codes("feet: x"), ["E102"]);
        let analysis = check("feet: x", &Rules::default());
        assert_eq!(analysis.diagnostics[0].notes, ["did you mean `feat`?"]);
        let any = Rules {
            types: None,
            ..Rules::default()
        };
        assert!(check("feet: x", &any).diagnostics.is_empty());
    }

    #[test]
    fn checks_the_scope() {
        let rules = Rules {
            scopes: Some(vec!["api".into(), "ui".into()]),
            require_scope: true,
            ..Rules::default()
        };
        assert_eq!(check("feat: x", &rules).diagnostics[0].code, "E103");
        assert_eq!(check("feat(apy): x", &rules).diagnostics[0].code, "E104");
        assert_eq!(codes("feat(ui button): x"), ["E105"]);
        assert!(check("feat(ui): x", &rules).diagnostics.is_empty());
    }

    #[test]
    fn checks_the_description() {
        assert_eq!(codes("feat: add thing."), ["E106"]);
        assert_eq!(codes("feat: Add thing"), ["W101"]);
        assert!(codes("feat: API support").is_empty());
        let long = format!("feat: {}", "x".repeat(95));
        assert_eq!(codes(&long), ["W102"]);
    }

    #[test]
    fn checks_the_body_and_footers() {
        assert_eq!(codes(&format!("feat: x\n\n{}", "y".repeat(101))), ["W103"]);
        assert_eq!(codes("feat: x\n\nBREAKING CHANGE: y\n\nprose"), ["E107"]);
        assert_eq!(codes("feat: x\n\nBreaking Change: y"), ["E108"]);
        assert_eq!(codes("feat: x\n\nBREAKING CHANGE #y"), ["W104"]);
        assert_eq!(codes("feat: x\n\nRefs:"), ["E109"]);
    }
}
