//! Recursive-descent parser turning the token stream into a [`Commit`].
//!
//! The header is parsed token by token. Everything after it is read as physical lines, grouped
//! into paragraphs. The footers are the longest run of trailing paragraphs that each open with a
//! `token: value` or `token #value` line; every paragraph before them is the body.

use std::ops::Range;

use crate::ast::{Body, Commit, Footer, Header, Ident, Separator, Text};
use crate::diagnostic::Diagnostic;
use crate::span::Span;
use crate::token::{Token, TokenKind};

pub struct Parsed {
    /// Absent when the header is not a conventional commit header at all.
    pub commit: Option<Commit>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(src: &str, tokens: Vec<Token>) -> Parsed {
    let mut parser = Parser {
        src,
        tokens,
        pos: 0,
        diagnostics: Vec::new(),
    };
    let commit = parser.commit();
    Parsed {
        commit,
        diagnostics: parser.diagnostics,
    }
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

/// A physical line after the header: its token range and its span without the terminator.
struct Line {
    tokens: Range<usize>,
    span: Span,
}

/// The `token` and separator that open a footer line, and the index of its first value token.
struct FooterStart {
    token: Ident,
    separator: Separator,
    value_at: usize,
}

impl<'a> Parser<'a> {
    fn cur(&self) -> Token {
        self.tokens[self.pos]
    }

    fn peek(&self, n: usize) -> Token {
        self.tokens[(self.pos + n).min(self.tokens.len() - 1)]
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.cur().kind == kind
    }

    fn at_line_end(&self) -> bool {
        matches!(self.cur().kind, TokenKind::Newline | TokenKind::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.cur();
        if token.kind != TokenKind::Eof {
            self.pos += 1;
        }
        token
    }

    fn eat(&mut self, kind: TokenKind) -> Option<Token> {
        self.at(kind).then(|| self.bump())
    }

    fn text(&self, span: Span) -> &'a str {
        span.slice(self.src)
    }

    /// The text of `span` without surrounding blanks, with a span narrowed to match.
    fn trimmed(&self, span: Span) -> Text {
        let raw = self.text(span);
        let text = raw.trim_start();
        let start = span.start + (raw.len() - text.len());
        let text = text.trim_end();
        Text::new(text, Span::new(start, start + text.len()))
    }

    fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    fn commit(&mut self) -> Option<Commit> {
        while self.eat(TokenKind::Newline).is_some() {}
        let header = self.header()?;
        let (body, footers) = if self.at(TokenKind::Eof) {
            (None, Vec::new())
        } else {
            self.sections()
        };
        let mut span = header.span;
        if let Some(body) = &body {
            span = span.to(body.span);
        }
        if let Some(footer) = footers.last() {
            span = span.to(footer.span);
        }
        Some(Commit {
            header,
            body,
            footers,
            span,
        })
    }

    fn header(&mut self) -> Option<Header> {
        let first = self.cur();
        if first.kind != TokenKind::Word {
            self.report(
                Diagnostic::error("E001", "expected a commit type at the start of the message")
                    .label(first.span, "expected a type such as `feat` or `fix`")
                    .help("a header is written `type(scope)!: description`"),
            );
            return None;
        }
        self.bump();
        let kind = Ident::new(self.text(first.span), first.span);
        let scope = self.scope();
        let bang = self.eat(TokenKind::Bang).map(|t| t.span);

        if self.at(TokenKind::Space) && self.peek(1).kind == TokenKind::Colon {
            let space = self.bump();
            self.report(
                Diagnostic::error("E004", "unexpected space before `:`")
                    .label(space.span, "remove this space"),
            );
        }
        let Some(colon) = self.eat(TokenKind::Colon) else {
            let at = self.cur();
            self.report(
                Diagnostic::error("E005", "expected `:` after the type")
                    .label(at.span, "expected `:` here")
                    .help("a header is written `type(scope)!: description`, e.g. `fix(parser): accept empty bodies`"),
            );
            return None;
        };
        if self.eat(TokenKind::Space).is_none() && !self.at_line_end() {
            self.report(
                Diagnostic::error("E006", "expected a space after `:`")
                    .label(Span::empty(colon.span.end), "insert a space here"),
            );
        }

        let start = self.cur().span.start;
        while !self.at_line_end() {
            self.bump();
        }
        let description = self.trimmed(Span::new(start, self.cur().span.start));
        if description.text.is_empty() {
            self.report(
                Diagnostic::error("E007", "the description must not be empty").label(
                    Span::empty(colon.span.end),
                    "expected a description after `: `",
                ),
            );
        }
        let end = description.span.end.max(colon.span.end);
        Some(Header {
            kind,
            scope,
            bang,
            description,
            span: Span::new(first.span.start, end),
        })
    }

    fn scope(&mut self) -> Option<Ident> {
        let open = self.eat(TokenKind::LParen)?;
        let start = self.cur().span.start;
        while !matches!(
            self.cur().kind,
            TokenKind::RParen | TokenKind::Colon | TokenKind::Newline | TokenKind::Eof
        ) {
            self.bump();
        }
        let span = Span::new(start, self.cur().span.start);
        let name = self.text(span);
        match self.eat(TokenKind::RParen) {
            Some(close) if name.trim().is_empty() => {
                self.report(
                    Diagnostic::error("E002", "the scope must not be empty")
                        .label(open.span.to(close.span), "empty scope")
                        .help("name the scope, e.g. `(parser)`, or drop the parentheses"),
                );
                None
            }
            Some(_) => Some(Ident::new(name, span)),
            None => {
                self.report(
                    Diagnostic::error("E003", "unclosed scope")
                        .label(Span::empty(span.end), "expected `)` here")
                        .label(open.span, "the scope opens here"),
                );
                (!name.trim().is_empty()).then(|| Ident::new(name, span))
            }
        }
    }

    /// Parses what follows the header line: the body and the footers.
    fn sections(&mut self) -> (Option<Body>, Vec<Footer>) {
        self.bump();
        if self.at(TokenKind::Eof) {
            return (None, Vec::new());
        }
        if self.eat(TokenKind::Newline).is_none() {
            let at = self.cur().span.start;
            self.report(
                Diagnostic::error(
                    "E008",
                    "the body must be separated from the header by a blank line",
                )
                .label(Span::empty(at), "expected a blank line before this line")
                .help("insert an empty line after the header"),
            );
        }
        let lines = self.lines();
        let paragraphs = paragraphs(&lines, &self.tokens);
        let first_footer = (0..paragraphs.len())
            .rev()
            .take_while(|&i| self.footer_start(&lines[paragraphs[i].start]).is_some())
            .last()
            .unwrap_or(paragraphs.len());
        let body = (first_footer > 0)
            .then(|| self.body(&lines[paragraphs[0].start..paragraphs[first_footer - 1].end]));
        let footers = paragraphs[first_footer..]
            .iter()
            .flat_map(|p| self.footers(&lines[p.clone()]))
            .collect();
        (body, footers)
    }

    fn lines(&mut self) -> Vec<Line> {
        let mut lines = Vec::new();
        loop {
            let first = self.pos;
            let start = self.cur().span.start;
            while !self.at_line_end() {
                self.bump();
            }
            lines.push(Line {
                tokens: first..self.pos,
                span: Span::new(start, self.cur().span.start),
            });
            if self.eat(TokenKind::Newline).is_none() || self.at(TokenKind::Eof) {
                return lines;
            }
        }
    }

    fn body(&self, lines: &[Line]) -> Body {
        let lines: Vec<Text> = lines
            .iter()
            .map(|line| {
                let text = self.text(line.span).trim_end();
                Text::new(
                    text,
                    Span::new(line.span.start, line.span.start + text.len()),
                )
            })
            .collect();
        let text = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let span = Span::new(lines[0].span.start, lines[lines.len() - 1].span.end);
        Body { text, lines, span }
    }

    /// Recognises `token: `, `token:` at the end of the line, `token #` and `BREAKING CHANGE: `.
    fn footer_start(&self, line: &Line) -> Option<FooterStart> {
        let tokens = &self.tokens[line.tokens.clone()];
        let first = tokens.first()?;
        let word = self.text(first.span);
        if first.kind != TokenKind::Word || !word.starts_with(|c: char| c.is_alphanumeric()) {
            return None;
        }
        let (span, next) = match tokens.get(1..3) {
            Some([space, second])
                if space.kind == TokenKind::Space
                    && second.kind == TokenKind::Word
                    && word.eq_ignore_ascii_case("BREAKING")
                    && self.text(second.span).eq_ignore_ascii_case("CHANGE") =>
            {
                (first.span.to(second.span), 3)
            }
            _ => (first.span, 1),
        };
        let kind = |i: usize| tokens.get(i).map(|t| t.kind);
        let (separator, value_at) = match (kind(next), kind(next + 1)) {
            (Some(TokenKind::Colon), Some(TokenKind::Space)) => (Separator::Colon, next + 2),
            (Some(TokenKind::Colon), None) => (Separator::Colon, next + 1),
            (Some(TokenKind::Space), Some(TokenKind::Hash)) => (Separator::Hash, next + 2),
            _ => return None,
        };
        Some(FooterStart {
            token: Ident::new(self.text(span), span),
            separator,
            value_at: line.tokens.start + value_at,
        })
    }

    /// Parses one paragraph of footers. Lines that do not open a footer continue the previous value.
    fn footers(&self, lines: &[Line]) -> Vec<Footer> {
        let mut footers: Vec<Footer> = Vec::new();
        for line in lines {
            match self.footer_start(line) {
                Some(start) => {
                    let from = if start.value_at < line.tokens.end {
                        self.tokens[start.value_at].span.start
                    } else {
                        line.span.end
                    };
                    footers.push(Footer {
                        token: start.token,
                        separator: start.separator,
                        value: self.trimmed(Span::new(from, line.span.end)),
                        span: line.span,
                    });
                }
                None => {
                    let Some(footer) = footers.last_mut() else {
                        continue;
                    };
                    let continuation = self.trimmed(line.span);
                    if footer.value.text.is_empty() {
                        footer.value = continuation;
                    } else {
                        footer.value.text.push('\n');
                        footer.value.text.push_str(&continuation.text);
                        footer.value.span = footer.value.span.to(continuation.span);
                    }
                    footer.span = footer.span.to(line.span);
                }
            }
        }
        footers
    }
}

/// Groups consecutive non-blank lines; returns index ranges into `lines`.
fn paragraphs(lines: &[Line], tokens: &[Token]) -> Vec<Range<usize>> {
    let is_blank = |line: &Line| {
        tokens[line.tokens.clone()]
            .iter()
            .all(|t| t.kind == TokenKind::Space)
    };
    let mut paragraphs = Vec::new();
    let mut open: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        match (is_blank(line), open) {
            (true, Some(start)) => {
                paragraphs.push(start..i);
                open = None;
            }
            (false, None) => open = Some(i),
            _ => {}
        }
    }
    if let Some(start) = open {
        paragraphs.push(start..lines.len());
    }
    paragraphs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_ok(src: &str) -> Commit {
        let parsed = parse(src, tokenize(src, true));
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        parsed.commit.expect("a commit")
    }

    fn codes(src: &str) -> Vec<&'static str> {
        parse(src, tokenize(src, true))
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn parses_a_full_header() {
        let commit = parse_ok("feat(api)!: send an email");
        let header = &commit.header;
        assert_eq!(header.kind.name, "feat");
        assert_eq!(header.scope.as_ref().map(|s| s.name.as_str()), Some("api"));
        assert_eq!(header.bang, Some(Span::new(9, 10)));
        assert_eq!(header.description.text, "send an email");
        assert_eq!(header.description.span, Span::new(12, 25));
        assert!(commit.body.is_none());
        assert!(commit.footers.is_empty());
    }

    #[test]
    fn splits_body_and_footers() {
        let src = "fix: prevent racing of requests\n\nIntroduce a request id.\n\nRemove timeouts which were\nobsolete.\n\nReviewed-by: Z\nRefs: #123\n";
        let commit = parse_ok(src);
        let body = commit.body.as_ref().expect("a body");
        assert_eq!(
            body.text,
            "Introduce a request id.\n\nRemove timeouts which were\nobsolete."
        );
        assert_eq!(body.lines.len(), 4);
        let footers: Vec<(&str, Separator, &str)> = commit
            .footers
            .iter()
            .map(|f| (f.token.name.as_str(), f.separator, f.value.text.as_str()))
            .collect();
        assert_eq!(
            footers,
            [
                ("Reviewed-by", Separator::Colon, "Z"),
                ("Refs", Separator::Colon, "#123")
            ]
        );
    }

    #[test]
    fn reads_hash_separators_and_multi_line_values() {
        let src = "feat: x\n\nBREAKING CHANGE: the config\nkey moved\nFixes #12";
        let commit = parse_ok(src);
        assert!(commit.body.is_none());
        assert_eq!(commit.footers.len(), 2);
        assert_eq!(commit.footers[0].value.text, "the config\nkey moved");
        assert!(commit.footers[0].is_breaking());
        assert_eq!(commit.footers[1].separator, Separator::Hash);
        assert_eq!(commit.footers[1].value.text, "12");
    }

    #[test]
    fn footers_are_the_trailing_footer_paragraphs() {
        let src = "feat: x\n\nNote: this is prose\n\nmore prose\n\nBREAKING CHANGE: a\n\nRefs: #1";
        let commit = parse_ok(src);
        assert_eq!(
            commit.body.as_ref().unwrap().text,
            "Note: this is prose\n\nmore prose"
        );
        assert_eq!(commit.footers.len(), 2);
    }

    #[test]
    fn a_dash_is_not_a_footer_token() {
        let commit = parse_ok("feat: x\n\n- #12 is unrelated");
        assert!(commit.footers.is_empty());
    }

    #[test]
    fn tolerates_leading_blank_lines_and_crlf() {
        let commit = parse_ok("\r\nfeat: x\r\n\r\nbody\r\n");
        assert_eq!(commit.body.as_ref().unwrap().text, "body");
    }

    #[test]
    fn reports_header_errors() {
        assert_eq!(codes(""), ["E001"]);
        assert_eq!(codes("feat(): x"), ["E002"]);
        assert_eq!(codes("feat(api: x"), ["E003"]);
        assert_eq!(codes("feat : x"), ["E004"]);
        assert_eq!(codes("Update README"), ["E005"]);
        assert_eq!(codes("feat:x"), ["E006"]);
        assert_eq!(codes("feat:"), ["E007"]);
        assert_eq!(codes("feat: x\nbody"), ["E008"]);
    }

    #[test]
    fn recovers_after_a_missing_blank_line() {
        let parsed = parse(
            "feat: x\nbody\n\nRefs: #1",
            tokenize("feat: x\nbody\n\nRefs: #1", true),
        );
        let commit = parsed.commit.unwrap();
        assert_eq!(commit.body.unwrap().text, "body");
        assert_eq!(commit.footers.len(), 1);
    }
}
