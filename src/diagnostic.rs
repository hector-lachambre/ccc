//! Diagnostics with source spans, and their rustc-style rendering.

use std::fmt::Write;

use serde::Serialize;

use crate::span::{SourceMap, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

/// A span with the sentence to print under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// `Exxx` for errors, `Wxxx` for warnings; stable, meant for documentation and filtering.
    pub code: &'static str,
    pub message: String,
    /// The first label is the primary one and gives the diagnostic its position.
    pub labels: Vec<Label>,
    /// `help:` lines printed after the snippet.
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn new(severity: Severity, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, code, message)
    }

    pub fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, code, message)
    }

    pub fn label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    pub fn help(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// Where the diagnostic points, when it has a label.
    pub fn primary_span(&self) -> Option<Span> {
        self.labels.first().map(|l| l.span)
    }
}

/// Renders diagnostics as text, with the source line and carets under the spans.
pub struct Renderer<'a> {
    src: &'a str,
    map: SourceMap<'a>,
    name: &'a str,
    palette: Palette,
}

struct Palette {
    error: &'static str,
    warning: &'static str,
    accent: &'static str,
    bold: &'static str,
    reset: &'static str,
}

impl Palette {
    const ANSI: Palette = Palette {
        error: "\x1b[1;31m",
        warning: "\x1b[1;33m",
        accent: "\x1b[1;34m",
        bold: "\x1b[1m",
        reset: "\x1b[0m",
    };

    const PLAIN: Palette = Palette {
        error: "",
        warning: "",
        accent: "",
        bold: "",
        reset: "",
    };
}

impl<'a> Renderer<'a> {
    /// `name` is what the location line shows, typically the file name or `<stdin>`.
    pub fn new(src: &'a str, name: &'a str, color: bool) -> Self {
        Self {
            src,
            map: SourceMap::new(src),
            name,
            palette: if color { Palette::ANSI } else { Palette::PLAIN },
        }
    }

    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let p = &self.palette;
        let (word, tint) = match diagnostic.severity {
            Severity::Error => ("error", p.error),
            Severity::Warning => ("warning", p.warning),
        };
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{tint}{word}[{}]{}{}: {}{}",
            diagnostic.code, p.reset, p.bold, diagnostic.message, p.reset
        );
        let Some(primary) = diagnostic.labels.first() else {
            return out;
        };

        let width = diagnostic
            .labels
            .iter()
            .map(|l| self.map.line_of(l.span.start))
            .max()
            .unwrap_or(1)
            .to_string()
            .len();
        let (line, col) = self.map.line_col(primary.span.start);
        let _ = writeln!(
            out,
            "{:width$}{}-->{} {}:{line}:{col}",
            "", p.accent, p.reset, self.name
        );
        let _ = writeln!(out, "{:width$} {}|{}", "", p.accent, p.reset);

        let mut shown = None;
        for (i, label) in diagnostic.labels.iter().enumerate() {
            let line = self.map.line_of(label.span.start);
            let text = self.map.line_text(line);
            if shown != Some(line) {
                let _ = writeln!(
                    out,
                    "{}{line:>width$} |{} {}",
                    p.accent,
                    p.reset,
                    expand(text)
                );
                shown = Some(line);
            }
            let line_start = self.map.line_start(line);
            let line_end = line_start + text.len();
            let from = label.span.start.min(line_end);
            let to = label.span.end.clamp(from, line_end);
            let pad: String = expand(&self.src[line_start..from])
                .chars()
                .map(|_| ' ')
                .collect();
            let carets = expand(&self.src[from..to]).chars().count().max(1);
            let (marker, tint) = if i == 0 { ("^", tint) } else { ("-", p.accent) };
            let message = if label.message.is_empty() {
                String::new()
            } else {
                format!(" {}", label.message)
            };
            let _ = writeln!(
                out,
                "{:width$} {}|{} {pad}{tint}{}{message}{}",
                "",
                p.accent,
                p.reset,
                marker.repeat(carets),
                p.reset
            );
        }

        if !diagnostic.notes.is_empty() {
            let _ = writeln!(out, "{:width$} {}|{}", "", p.accent, p.reset);
            for note in &diagnostic.notes {
                let _ = writeln!(
                    out,
                    "{:width$} {}={} {}help{}: {note}",
                    "", p.accent, p.reset, p.bold, p.reset
                );
            }
        }
        out
    }
}

fn expand(text: &str) -> String {
    text.replace('\t', "    ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_snippet_with_carets() {
        let src = "feat(api: add thing\n\nbody";
        let diagnostic = Diagnostic::error("E003", "unclosed scope")
            .label(Span::empty(8), "expected `)` here")
            .label(Span::new(4, 5), "the scope opens here")
            .help("close the scope: `feat(api): add thing`");
        let rendered = Renderer::new(src, "msg.txt", false).render(&diagnostic);
        let expected = "\
error[E003]: unclosed scope
 --> msg.txt:1:9
  |
1 | feat(api: add thing
  |         ^ expected `)` here
  |     - the scope opens here
  |
  = help: close the scope: `feat(api): add thing`
";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn points_past_the_end_of_a_line() {
        let src = "feat:";
        let diagnostic = Diagnostic::error("E007", "empty").label(Span::empty(5), "here");
        let rendered = Renderer::new(src, "<stdin>", false).render(&diagnostic);
        assert!(
            rendered.contains("1 | feat:\n  |      ^ here\n"),
            "{rendered}"
        );
    }
}
