//! Variable interpolation and .env file loading following the Compose Specification.

use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum InterpolationError {
    #[error("required variable '{0}' is not set: {1}")]
    MissingRequiredVariable(String, String),
    #[error("syntax error in variable interpolation: '{0}'")]
    SyntaxError(String),
}

/// Interpolates `${VAR}` expressions in `input` using provided `env` map.
///
/// Supported formats:
/// - `$$`: literal `$`
/// - `${VAR}`: direct value of `VAR` or empty string if not found
/// - `${VAR:-default}`: `default` if `VAR` is unset or empty
/// - `${VAR-default}`: `default` only if `VAR` is unset
/// - `${VAR:?error}`: returns error if `VAR` is unset or empty
/// - `${VAR?error}`: returns error only if `VAR` is unset
pub fn interpolate_string(
    input: &str,
    env: &HashMap<String, String>,
) -> Result<String, InterpolationError> {
    // Regex matching:
    // 1. "$$" (escape)
    // 2. "${...}"
    // 3. "$VAR"
    let re = Regex::new(r"(\$\$|\$\{([a-zA-Z0-9_]+)(?::?[-?]([^}]*))?\}|\$([a-zA-Z0-9_]+))")
        .map_err(|e| InterpolationError::SyntaxError(e.to_string()))?;

    let mut result = String::with_capacity(input.len());
    let mut last_idx = 0;

    for mat in re.find_iter(input) {
        result.push_str(&input[last_idx..mat.start()]);
        let token = mat.as_str();

        if token == "$$" {
            result.push('$');
        } else if token.starts_with("${") && token.ends_with('}') {
            let inner = &token[2..token.len() - 1];
            let interpolated = resolve_braced_var(inner, env)?;
            result.push_str(&interpolated);
        } else if token.starts_with('$') {
            let var_name = &token[1..];
            if let Some(val) = env.get(var_name) {
                result.push_str(val);
            }
        }
        last_idx = mat.end();
    }

    result.push_str(&input[last_idx..]);
    Ok(result)
}

fn resolve_braced_var(
    inner: &str,
    env: &HashMap<String, String>,
) -> Result<String, InterpolationError> {
    if let Some(idx) = inner.find(":-") {
        let var_name = &inner[..idx];
        let default_val = &inner[idx + 2..];
        match env.get(var_name) {
            Some(val) if !val.is_empty() => Ok(val.clone()),
            _ => Ok(default_val.to_string()),
        }
    } else if let Some(idx) = inner.find('-') {
        let var_name = &inner[..idx];
        let default_val = &inner[idx + 1..];
        match env.get(var_name) {
            Some(val) => Ok(val.clone()),
            None => Ok(default_val.to_string()),
        }
    } else if let Some(idx) = inner.find(":?") {
        let var_name = &inner[..idx];
        let err_msg = &inner[idx + 2..];
        match env.get(var_name) {
            Some(val) if !val.is_empty() => Ok(val.clone()),
            _ => Err(InterpolationError::MissingRequiredVariable(
                var_name.to_string(),
                if err_msg.is_empty() {
                    "variable is required".to_string()
                } else {
                    err_msg.to_string()
                },
            )),
        }
    } else if let Some(idx) = inner.find('?') {
        let var_name = &inner[..idx];
        let err_msg = &inner[idx + 1..];
        match env.get(var_name) {
            Some(val) => Ok(val.clone()),
            None => Err(InterpolationError::MissingRequiredVariable(
                var_name.to_string(),
                if err_msg.is_empty() {
                    "variable is required".to_string()
                } else {
                    err_msg.to_string()
                },
            )),
        }
    } else {
        // Simple ${VAR}
        Ok(env.get(inner).cloned().unwrap_or_default())
    }
}

/// Parses the contents of a .env file into a key-value map.
pub fn parse_env_content(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key_part, val_part)) = trimmed.split_once('=') {
            let key = key_part.trim().to_string();
            let mut val = val_part.trim().to_string();

            // Strip enclosing quotes
            if (val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\''))
            {
                if val.len() >= 2 {
                    val = val[1..val.len() - 1].to_string();
                }
            } else if let Some(idx) = val.find(" #") {
                // Inline comment on unquoted value
                val = val[..idx].trim().to_string();
            }

            map.insert(key, val);
        }
    }
    map
}

/// Loads a .env file from disk if it exists.
pub fn load_env_file(path: &Path) -> HashMap<String, String> {
    if let Ok(content) = fs::read_to_string(path) {
        parse_env_content(&content)
    } else {
        HashMap::new()
    }
}

/// Combines process environment variables with .env file (process env takes precedence).
pub fn build_effective_env(env_file_path: Option<&Path>) -> HashMap<String, String> {
    let mut effective = if let Some(path) = env_file_path {
        load_env_file(path)
    } else {
        HashMap::new()
    };

    for (k, v) in std::env::vars() {
        effective.insert(k, v);
    }

    effective
}
