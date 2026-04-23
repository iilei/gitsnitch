use std::env;
use std::io::{self, IsTerminal};
use std::path::Path;

use minijinja::Environment;
use serde::Serialize;

use super::{AppError, RenderOutput};

const TEXT_REPORT_TEMPLATE: &str = include_str!("templates/report_decorative_text.jinja2");

pub(crate) struct EmitOptions<'a> {
    pub(crate) output_format: RenderOutput,
    pub(crate) gitsnitch_json_path: Option<&'a Path>,
}

#[derive(Debug, Serialize)]
struct TerminalRenderContext {
    supports_color: bool,
    is_ci: bool,
}

pub(crate) fn terminal_supports_color_from_inputs(
    no_color_present: bool,
    term: Option<&str>,
    clicolor_force: Option<&str>,
    clicolor: Option<&str>,
    stdout_is_terminal: bool,
    terminal_is_ansi_compatible: bool,
) -> bool {
    if !stdout_is_terminal {
        return false;
    }

    // Fail closed for decorative ANSI output: if compatibility is unclear,
    // behave as if NO_COLOR were set.
    if !terminal_is_ansi_compatible {
        return false;
    }

    if no_color_present {
        return false;
    }

    if term.is_some_and(|value| value.eq_ignore_ascii_case("dumb")) {
        return false;
    }

    if clicolor_force.is_some_and(|value| value != "0") {
        return true;
    }

    if clicolor.is_some_and(|value| value == "0") {
        return false;
    }

    true
}

pub(crate) fn terminal_is_ansi_compatible_from_inputs(
    term: Option<&str>,
    term_program: Option<&str>,
    wt_session_present: bool,
    ansicon_present: bool,
    conemuansi: Option<&str>,
) -> bool {
    if term.is_some_and(|value| value.eq_ignore_ascii_case("dumb")) {
        return false;
    }

    if wt_session_present || ansicon_present {
        return true;
    }

    if conemuansi.is_some_and(|value| value.eq_ignore_ascii_case("ON")) {
        return true;
    }

    if term_program.is_some_and(|value| value.eq_ignore_ascii_case("vscode")) {
        return true;
    }

    term.is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        [
            "xterm", "ansi", "vt100", "vt220", "screen", "tmux", "rxvt", "linux", "cygwin", "msys",
        ]
        .iter()
        .any(|needle| value.contains(needle))
    })
}

pub(crate) fn detect_terminal_supports_color() -> bool {
    let no_color_present = env::var_os("NO_COLOR").is_some();
    let term = env::var("TERM").ok();
    let term_program = env::var("TERM_PROGRAM").ok();
    let clicolor_force = env::var("CLICOLOR_FORCE").ok();
    let clicolor = env::var("CLICOLOR").ok();
    let wt_session_present = env::var_os("WT_SESSION").is_some();
    let ansicon_present = env::var_os("ANSICON").is_some();
    let conemuansi = env::var("ConEmuANSI").ok();

    let terminal_is_ansi_compatible = terminal_is_ansi_compatible_from_inputs(
        term.as_deref(),
        term_program.as_deref(),
        wt_session_present,
        ansicon_present,
        conemuansi.as_deref(),
    );

    terminal_supports_color_from_inputs(
        no_color_present,
        term.as_deref(),
        clicolor_force.as_deref(),
        clicolor.as_deref(),
        io::stdout().is_terminal(),
        terminal_is_ansi_compatible,
    )
}

fn serialize_json_report<T: Serialize>(
    report: &T,
    compact_output: bool,
) -> Result<String, AppError> {
    (if compact_output {
        serde_json::to_string(report)
    } else {
        serde_json::to_string_pretty(report)
    })
    .map_err(|error| AppError::Message(format!("failed to serialize report as JSON: {error}")))
}

fn emit_json_report<T: Serialize>(report: &T, compact_output: bool) -> Result<(), AppError> {
    let serialized = serialize_json_report(report, compact_output)?;
    println!("{serialized}");

    Ok(())
}

fn emit_text_report<T: Serialize>(supports_color: bool, report: &T) -> Result<(), AppError> {
    let terminal = TerminalRenderContext {
        supports_color,
        is_ci: env::var_os("CI").is_some(),
    };
    let environment = Environment::new();
    let rendered = environment
        .render_str(
            TEXT_REPORT_TEMPLATE,
            minijinja::context!(report => report, terminal => terminal),
        )
        .map_err(|error| {
            AppError::Message(format!("failed to render plain-text report: {error}"))
        })?;

    println!("{rendered}");

    Ok(())
}

pub(crate) fn emit_report_output<T: Serialize>(
    report: &T,
    emit_options: &EmitOptions<'_>,
) -> Result<(), AppError> {
    if let Some(path) = emit_options.gitsnitch_json_path {
        let serialized = serialize_json_report(report, false)?;
        std::fs::write(path, format!("{serialized}\n")).map_err(|error| {
            AppError::Message(format!(
                "failed to write --gitsnitch-json output to '{}': {error}",
                path.display()
            ))
        })?;
    }

    match emit_options.output_format {
        RenderOutput::Json => emit_json_report(report, false),
        RenderOutput::JsonCompact => emit_json_report(report, true),
        RenderOutput::TextPlain => emit_text_report(false, report),
        RenderOutput::TextDecorative => {
            let supports_color = detect_terminal_supports_color();
            emit_text_report(supports_color, report)
        }
    }
}
