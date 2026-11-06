//! A compiler for [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) messages.
//!
//! The pipeline is the classic one. [`lexer`] turns the message into tokens, [`parser`] builds the
//! [`ast::Commit`], [`sema`] checks it against the specification and the configured
//! [`sema::Rules`], and [`emit`] serialises the result. Every stage reports
//! [`diagnostic::Diagnostic`]s with byte spans into the original text, which
//! [`diagnostic::Renderer`] prints rustc-style.
//!
//! ```
//! use ccc::{compile, Options};
//! use ccc::sema::Bump;
//!
//! let compilation = compile("feat(parser): accept multi-line footers", &Options::default());
//! assert!(compilation.is_ok());
//! assert_eq!(compilation.bump, Bump::Minor);
//! ```

pub mod ast;
pub mod diagnostic;
pub mod emit;
pub mod lexer;
pub mod parser;
pub mod sema;
pub mod span;
pub mod token;

use ast::Commit;
use diagnostic::{Diagnostic, Severity};
use sema::{Bump, Rules};

/// Everything that drives a compilation.
#[derive(Debug, Clone)]
pub struct Options {
    /// Drop lines starting with `#`, as git does when it reads `COMMIT_EDITMSG`.
    pub strip_comment_lines: bool,
    pub rules: Rules,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            strip_comment_lines: true,
            rules: Rules::default(),
        }
    }
}

/// The outcome of compiling one message.
#[derive(Debug, Clone)]
pub struct Compilation {
    /// The parsed message, absent when the header could not be parsed at all.
    pub commit: Option<Commit>,
    /// Every diagnostic, ordered by position in the source.
    pub diagnostics: Vec<Diagnostic>,
    pub breaking: bool,
    pub bump: Bump,
}

impl Compilation {
    /// True when no diagnostic is an error.
    pub fn is_ok(&self) -> bool {
        !self.diagnostics.iter().any(Diagnostic::is_error)
    }

    pub fn errors(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }

    pub fn warnings(&self) -> usize {
        self.diagnostics.len() - self.errors()
    }

    /// Turns every warning into an error.
    pub fn deny_warnings(&mut self) {
        for diagnostic in &mut self.diagnostics {
            diagnostic.severity = Severity::Error;
        }
    }
}

/// Runs the whole pipeline on `src`.
pub fn compile(src: &str, options: &Options) -> Compilation {
    let tokens = lexer::tokenize(src, options.strip_comment_lines);
    let parsed = parser::parse(src, tokens);
    let mut diagnostics = parsed.diagnostics;
    let (breaking, bump) = match &parsed.commit {
        Some(commit) => {
            let analysis = sema::analyze(commit, src, &options.rules);
            diagnostics.extend(analysis.diagnostics);
            (analysis.breaking, analysis.bump)
        }
        None => (false, Bump::None),
    };
    diagnostics.sort_by_key(|d| d.primary_span().map_or(0, |s| s.start));
    Compilation {
        commit: parsed.commit,
        diagnostics,
        breaking,
        bump,
    }
}
