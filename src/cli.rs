use std::ffi::OsStr;

use super::{AppError, Args, DEFAULT_ENV_PREFIX};

pub(crate) fn validate_custom_meta(entries: &[String]) -> Result<(), AppError> {
    for entry in entries {
        let Some((key, value)) = entry.split_once('=') else {
            return Err(AppError::Message(format!(
                "invalid --custom-meta entry '{entry}': expected key=value"
            )));
        };

        if key.trim().is_empty() {
            return Err(AppError::Message(format!(
                "invalid --custom-meta entry '{entry}': key cannot be empty"
            )));
        }

        if value.trim().is_empty() {
            return Err(AppError::Message(format!(
                "invalid --custom-meta entry '{entry}': value cannot be empty"
            )));
        }
    }

    Ok(())
}

pub(crate) fn validate_env_resolution_mode(args: &Args) -> Result<(), AppError> {
    if !args.remap_env_var.is_empty() && args.env_prefix != DEFAULT_ENV_PREFIX {
        return Err(AppError::Message(
            "--remap-env-var is mutually exclusive with non-default --env-prefix; use default GITSNITCH_ prefix when remapping"
                .to_owned(),
        ));
    }

    Ok(())
}

pub(crate) fn validate_gitsnitch_json_path(args: &Args) -> Result<(), AppError> {
    let Some(path) = &args.gitsnitch_json else {
        return Ok(());
    };

    if path.as_os_str() == OsStr::new("-") {
        return Err(AppError::Message(
            "--gitsnitch-json does not accept '-' ; provide a real file path".to_owned(),
        ));
    }

    Ok(())
}
