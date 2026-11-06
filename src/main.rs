use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use ccc::diagnostic::Renderer;
use ccc::emit::{self, Report};
use ccc::sema::Rules;
use ccc::{Options, compile};
use clap::{Parser, ValueEnum};

/// A compiler for Conventional Commits messages.
///
/// Reads a commit message, parses it, checks it against the specification and the configured
/// rules, and reports diagnostics the way a compiler would. Exits with 0 when the message is
/// accepted, 1 when it is rejected, 2 on a usage or I/O error.
#[derive(Parser)]
#[command(name = "ccc", version, about, long_about)]
struct Cli {
    /// File holding the message; reads standard input when omitted or `-`
    file: Option<PathBuf>,

    /// Take the message from the command line instead of a file
    #[arg(short, long, conflicts_with = "file", value_name = "TEXT")]
    message: Option<String>,

    /// What to write on standard output
    #[arg(long, value_enum, value_name = "WHAT")]
    emit: Option<Emit>,

    /// Allowed types, comma-separated; `any` lifts the restriction
    #[arg(
        short,
        long,
        value_name = "LIST",
        default_value = "feat,fix,docs,style,refactor,perf,test,build,ci,chore,revert"
    )]
    types: String,

    /// Allowed scopes, comma-separated; any scope is accepted when omitted
    #[arg(short, long, value_name = "LIST")]
    scopes: Option<String>,

    /// Reject messages without a scope
    #[arg(long)]
    require_scope: bool,

    /// Maximum header length in characters; 0 disables the check
    #[arg(long, value_name = "N", default_value_t = 100)]
    max_header: usize,

    /// Maximum body line length in characters; 0 disables the check
    #[arg(long, value_name = "N", default_value_t = 100)]
    max_body_line: usize,

    /// Accept a description that starts with a capital letter
    #[arg(long)]
    allow_capitalized: bool,

    /// Keep lines starting with `#` instead of dropping them like git does
    #[arg(long)]
    keep_comments: bool,

    /// Treat warnings as errors
    #[arg(short = 'D', long)]
    deny_warnings: bool,

    /// Write nothing on standard error, only set the exit status
    #[arg(short, long)]
    quiet: bool,

    /// When to colour diagnostics
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = Color::Auto)]
    color: Color,
}

#[derive(Clone, Copy, ValueEnum)]
enum Emit {
    /// The parsed message as JSON, when it is accepted
    Ast,
    /// Acceptance, bump, parsed message and diagnostics as JSON, always
    Report,
    /// The version bump the message calls for: major, minor, patch or none
    Bump,
    /// The message rewritten in canonical form, when it is accepted
    Canonical,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Color {
    Auto,
    Always,
    Never,
}

impl Cli {
    fn rules(&self) -> Rules {
        Rules {
            types: parse_list(&self.types),
            scopes: self.scopes.as_deref().and_then(parse_list),
            require_scope: self.require_scope,
            max_header_length: (self.max_header > 0).then_some(self.max_header),
            max_body_line_length: (self.max_body_line > 0).then_some(self.max_body_line),
            lowercase_description: !self.allow_capitalized,
        }
    }

    fn color(&self) -> bool {
        match self.color {
            Color::Always => true,
            Color::Never => false,
            Color::Auto => io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    /// The message text and the name to show in diagnostics.
    fn input(&self) -> io::Result<(String, String)> {
        if let Some(message) = &self.message {
            return Ok(("<message>".into(), message.clone()));
        }
        match &self.file {
            Some(path) if path.as_os_str() != "-" => {
                let text = fs::read_to_string(path)
                    .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
                Ok((path.display().to_string(), text))
            }
            _ => {
                let mut text = String::new();
                io::stdin().read_to_string(&mut text)?;
                Ok(("<stdin>".into(), text))
            }
        }
    }
}

/// `a, b,c` to `["a", "b", "c"]`; `any` to `None`.
fn parse_list(list: &str) -> Option<Vec<String>> {
    if list.trim() == "any" {
        return None;
    }
    Some(
        list.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
    )
}

fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (name, src) = match cli.input() {
        Ok(input) => input,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };

    let options = Options {
        strip_comment_lines: !cli.keep_comments,
        rules: cli.rules(),
    };
    let mut compilation = compile(&src, &options);
    if cli.deny_warnings {
        compilation.deny_warnings();
    }
    let ok = compilation.is_ok();

    if !cli.quiet {
        let color = cli.color();
        let renderer = Renderer::new(&src, &name, color);
        let mut err = io::stderr().lock();
        for diagnostic in &compilation.diagnostics {
            let _ = writeln!(err, "{}", renderer.render(diagnostic));
        }
        let (errors, warnings) = (compilation.errors(), compilation.warnings());
        let (red, yellow, reset) = if color {
            ("\x1b[1;31m", "\x1b[1;33m", "\x1b[0m")
        } else {
            ("", "", "")
        };
        if errors > 0 {
            let also = if warnings > 0 {
                format!("; {} emitted", plural(warnings, "warning"))
            } else {
                String::new()
            };
            let _ = writeln!(
                err,
                "{red}error{reset}: the message was rejected due to {}{also}",
                plural(errors, "error")
            );
        } else if warnings > 0 {
            let _ = writeln!(
                err,
                "{yellow}warning{reset}: {} emitted",
                plural(warnings, "warning")
            );
        }
    }

    if let Some(what) = cli.emit {
        let text = match (what, ok, &compilation.commit) {
            (Emit::Report, _, _) => Some(emit::json(&Report::new(&compilation))),
            (Emit::Ast, true, Some(commit)) => Some(emit::json(commit)),
            (Emit::Bump, true, _) => Some(compilation.bump.to_string()),
            (Emit::Canonical, true, Some(commit)) => Some(emit::canonical(commit)),
            _ => None,
        };
        if let Some(text) = text {
            println!("{}", text.trim_end());
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
