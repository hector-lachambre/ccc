//! The syntax tree of a commit message. Every node keeps its [`Span`] into the source text.

use serde::Serialize;

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Commit {
    pub header: Header,
    pub body: Option<Body>,
    pub footers: Vec<Footer>,
    pub span: Span,
}

impl Commit {
    /// True when the message carries a breaking change, through `!` in the header or a
    /// `BREAKING CHANGE` footer.
    pub fn is_breaking(&self) -> bool {
        self.header.bang.is_some() || self.footers.iter().any(Footer::is_breaking)
    }

    pub fn breaking_changes(&self) -> impl Iterator<Item = &Footer> {
        self.footers.iter().filter(|f| f.is_breaking())
    }
}

/// The first line: `type(scope)!: description`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Header {
    #[serde(rename = "type")]
    pub kind: Ident,
    pub scope: Option<Ident>,
    /// The `!` marking a breaking change, when present.
    pub bang: Option<Span>,
    pub description: Text,
    pub span: Span,
}

/// A type, a scope or a footer token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            span,
        }
    }

    /// Exactly `BREAKING CHANGE` or `BREAKING-CHANGE`, the two forms the specification accepts.
    pub fn is_breaking(&self) -> bool {
        matches!(self.name.as_str(), "BREAKING CHANGE" | "BREAKING-CHANGE")
    }

    /// A breaking-change token in any case or spacing (`Breaking change`, `breaking-change`...).
    /// Semantic analysis rejects every form that [`Ident::is_breaking`] does not accept.
    pub fn looks_breaking(&self) -> bool {
        let words: Vec<String> = self
            .name
            .split(|c: char| c == '-' || c.is_whitespace())
            .filter(|w| !w.is_empty())
            .map(str::to_ascii_uppercase)
            .collect();
        words == ["BREAKING", "CHANGE"]
    }
}

/// Free text with its position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Text {
    pub text: String,
    pub span: Span,
}

impl Text {
    pub fn new(text: impl Into<String>, span: Span) -> Self {
        Self {
            text: text.into(),
            span,
        }
    }
}

/// The paragraphs between the header and the footers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Body {
    /// The lines joined with `\n`, trailing blanks removed.
    pub text: String,
    /// Each physical line with its exact span, blank lines included.
    #[serde(skip)]
    pub lines: Vec<Text>,
    pub span: Span,
}

/// A `token: value` or `token #value` trailer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Footer {
    pub token: Ident,
    pub separator: Separator,
    pub value: Text,
    pub span: Span,
}

impl Footer {
    pub fn is_breaking(&self) -> bool {
        self.token.is_breaking()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Separator {
    /// `: `
    Colon,
    /// ` #`
    Hash,
}

impl Separator {
    pub const fn as_str(self) -> &'static str {
        match self {
            Separator::Colon => ": ",
            Separator::Hash => " #",
        }
    }
}
