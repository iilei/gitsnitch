pub fn resolve_violation_severity_exit_switch(
    cli_override: Option<bool>,
    config_value: Option<bool>,
) -> bool {
    cli_override.or(config_value).unwrap_or(false)
}

pub fn resolve_violation_exit_code(
    violation_severity_as_exit_code: bool,
    violation_severities: &[u8],
) -> i32 {
    if !violation_severity_as_exit_code {
        return 0;
    }

    violation_severities
        .iter()
        .max()
        .map_or(0, |max_severity| i32::from(*max_severity))
}
