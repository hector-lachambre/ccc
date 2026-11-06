//! Turns the message into a flat token stream.
//!
//! The lexer is deliberately shallow: it only separates the characters the grammar cares about
//! (`(`, `)`, `!`, `:`, `#`, blanks and newlines) from runs of words and punctuation. The parser
//! decides what a token means from where it stands.

use crate::span::Span;
use crate::token::{Token, TokenKind};

/// Tokenises `src`. With `strip_comment_lines`, lines starting with `#` are dropped entirely,
/// the way git drops them from `COMMIT_EDITMSG`. The stream always ends with [`TokenKind::Eof`].
pub fn tokenize(src: &str, strip_comment_lines: bool) -> Vec<Token> {
    let mut lexer = Lexer {
        src,
        pos: 0,
        at_line_start: true,
        strip_comment_lines,
    };
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token();
        let done = token.kind == TokenKind::Eof;
        tokens.push(token);
        if done {
            return tokens;
        }
    }
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    at_line_start: bool,
    strip_comment_lines: bool,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn eat_while(&mut self, pred: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&pred) {
            self.bump();
        }
    }

    fn next_token(&mut self) -> Token {
        while self.at_line_start && self.strip_comment_lines && self.peek() == Some('#') {
            self.eat_while(|c| c != '\n');
            self.bump();
        }
        let start = self.pos;
        let Some(c) = self.bump() else {
            return Token::new(TokenKind::Eof, Span::empty(start));
        };
        let kind = match c {
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '!' => TokenKind::Bang,
            ':' => TokenKind::Colon,
            '#' => TokenKind::Hash,
            '\n' => TokenKind::Newline,
            '\r' if self.peek() == Some('\n') => {
                self.bump();
                TokenKind::Newline
            }
            c if is_blank(c) => {
                self.eat_while(is_blank);
                TokenKind::Space
            }
            c if is_word(c) => {
                self.eat_while(is_word);
                TokenKind::Word
            }
            _ => {
                self.eat_while(|c| !is_delimiter(c));
                TokenKind::Punct
            }
        };
        self.at_line_start = kind == TokenKind::Newline;
        Token::new(kind, Span::new(start, self.pos))
    }
}

fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

fn is_delimiter(c: char) -> bool {
    matches!(c, '(' | ')' | '!' | ':' | '#' | '\n' | '\r') || is_blank(c) || is_word(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenKind::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src, true).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_a_header() {
        assert_eq!(
            kinds("feat(api)!: add thing"),
            [
                Word, LParen, Word, RParen, Bang, Colon, Space, Word, Space, Word, Eof
            ]
        );
    }

    #[test]
    fn groups_punctuation_and_keeps_newlines() {
        assert_eq!(
            kinds("fix: see https://x.y\r\n\nFixes #1"),
            [
                Word, Colon, Space, Word, Space, Word, Colon, Punct, Word, Punct, Word, Newline,
                Newline, Word, Space, Hash, Word, Eof
            ]
        );
    }

    #[test]
    fn drops_comment_lines_with_their_newline() {
        let tokens = tokenize("feat: a\n# comment\n\nbody", true);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            [Word, Colon, Space, Word, Newline, Newline, Word, Eof]
        );
        assert_eq!(tokens[6].span, Span::new(19, 23));
    }

    #[test]
    fn keeps_comment_lines_on_request() {
        let kinds: Vec<_> = tokenize("# a\nfeat: b", false)
            .into_iter()
            .map(|t| t.kind)
            .collect();
        assert_eq!(
            kinds,
            [Hash, Space, Word, Newline, Word, Colon, Space, Word, Eof]
        );
    }

    #[test]
    fn spans_cover_the_source() {
        let src = "feat: héllo";
        let tokens = tokenize(src, true);
        let rebuilt: String = tokens.iter().map(|t| t.span.slice(src)).collect();
        assert_eq!(rebuilt, src);
    }
}
