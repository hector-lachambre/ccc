//! The tokens the lexer produces.

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A run of letters, digits, `_` and `-`: a type, a scope, a footer token, a word of prose.
    Word,
    LParen,
    RParen,
    Bang,
    Colon,
    Hash,
    /// A run of spaces and tabs.
    Space,
    /// `\n` or `\r\n`.
    Newline,
    /// Any other run of characters.
    Punct,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
