//! Variable interpolation and .env file loading following the Compose Specification.

use serde_yaml::Value;
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

/// Interpolates `$VAR` / `${...}` expressions in `input` using the provided `env` map.
///
/// Supported formats:
/// - `$$`: literal `$`
/// - `$VAR`, `${VAR}`: value of `VAR`, or empty string if unset
/// - `${VAR:-default}` / `${VAR-default}`: `default` if `VAR` is unset or empty / unset
/// - `${VAR:?error}` / `${VAR?error}`: error if `VAR` is unset or empty / unset
/// - `${VAR:+alt}` / `${VAR+alt}`: `alt` if `VAR` is set and non-empty / set
///
/// Defaults and alternatives may themselves contain interpolations (`${A:-${B}}`).
pub fn interpolate_string(
    input: &str,
    env: &HashMap<String, String>,
) -> Result<String, InterpolationError> {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(pos) = rest.find('$') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];

        if let Some(stripped) = after.strip_prefix('$') {
            out.push('$');
            rest = stripped;
        } else if let Some(braced) = after.strip_prefix('{') {
            let end = find_closing_brace(braced)
                .ok_or_else(|| InterpolationError::SyntaxError(rest.to_string()))?;
            out.push_str(&resolve_braced(&braced[..end], env)?);
            rest = &braced[end + 1..];
        } else {
            let name_len = var_name_len(after);
            if name_len == 0 {
                out.push('$');
                rest = after;
            } else {
                if let Some(val) = env.get(&after[..name_len]) {
                    out.push_str(val);
                }
                rest = &after[name_len..];
            }
        }
    }

    out.push_str(rest);
    Ok(out)
}

/// Recursively interpolates every string scalar of a parsed YAML tree (mapping keys excluded).
pub fn interpolate_value(
    value: &mut Value,
    env: &HashMap<String, String>,
) -> Result<(), InterpolationError> {
    match value {
        Value::String(s) => *s = interpolate_string(s, env)?,
        Value::Sequence(seq) => {
            for item in seq {
                interpolate_value(item, env)?;
            }
        }
        Value::Mapping(map) => {
            for (_, item) in map.iter_mut() {
                interpolate_value(item, env)?;
            }
        }
        Value::Tagged(tagged) => interpolate_value(&mut tagged.value, env)?,
        _ => {}
    }
    Ok(())
}

/// Length of a leading `[A-Za-z_][A-Za-z0-9_]*` identifier.
fn var_name_len(s: &str) -> usize {
    let mut chars = s.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return 0,
    }
    chars
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// Index of the `}` that closes a `${`, honoring nested `${...}` in operands.
fn find_closing_brace(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'$' if bytes.get(i + 1) == Some(&b'{') => {
                depth += 1;
                i += 1;
            }
            b'}' if depth == 0 => return Some(i),
            b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

fn resolve_braced(inner: &str, env: &HashMap<String, String>) -> Result<String, InterpolationError> {
    let name_len = var_name_len(inner);
    if name_len == 0 {
        return Err(InterpolationError::SyntaxError(format!("${{{}}}", inner)));
    }
    let name = &inner[..name_len];
    let rest = &inner[name_len..];
    let value = env.get(name);

    if rest.is_empty() {
        return Ok(value.cloned().unwrap_or_default());
    }

    // ":" variants treat an empty value like an unset one.
    let (empty_is_unset, op, operand) = match rest.strip_prefix(':') {
        Some(r) => (true, r.chars().next(), r.get(1..).unwrap_or("")),
        None => (false, rest.chars().next(), rest.get(1..).unwrap_or("")),
    };
    let missing = match value {
        None => true,
        Some(v) => empty_is_unset && v.is_empty(),
    };

    match op {
        Some('-') if missing => interpolate_string(operand, env),
        Some('-') => Ok(value.cloned().unwrap_or_default()),
        Some('+') if missing => Ok(String::new()),
        Some('+') => interpolate_string(operand, env),
        Some('?') if missing => {
            let msg = interpolate_string(operand, env)?;
            Err(InterpolationError::MissingRequiredVariable(
                name.to_string(),
                if msg.is_empty() { "variable is required".to_string() } else { msg },
            ))
        }
        Some('?') => Ok(value.cloned().unwrap_or_default()),
        _ => Err(InterpolationError::SyntaxError(format!("${{{}}}", inner))),
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
        let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);

        if let Some((key_part, val_part)) = trimmed.split_once('=') {
            let key = key_part.trim().to_string();
            let mut val = val_part.trim().to_string();

            // Strip enclosing quotes
            if val.len() >= 2
                && ((val.starts_with('"') && val.ends_with('"'))
                    || (val.starts_with('\'') && val.ends_with('\'')))
            {
                val = val[1..val.len() - 1].to_string();
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
