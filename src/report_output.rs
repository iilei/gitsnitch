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
) -> bool {
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

    stdout_is_terminal
}

pub(crate) fn detect_terminal_supports_color() -> bool {
    let no_color_present = env::var_os("NO_COLOR").is_some();
    let term = env::var("TERM").ok();
    let clicolor_force = env::var("CLICOLOR_FORCE").ok();
    let clicolor = env::var("CLICOLOR").ok();

    terminal_supports_color_from_inputs(
        no_color_present,
        term.as_deref(),
        clicolor_force.as_deref(),
        clicolor.as_deref(),
        io::stdout().is_terminal(),
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
        RenderOutput::TextDecorative => emit_text_report(detect_terminal_supports_color(), report),
    }
}
