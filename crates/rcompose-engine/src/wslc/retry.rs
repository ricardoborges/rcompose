//! Retry runner with backoff for transient WSL container preview locks.

use crate::engine::EngineError;
use std::future::Future;
use std::time::Duration;

pub const TRANSIENT_MARKERS: &[&str] = &[
    "ERROR_ALREADY_EXISTS",
    "ERROR_SHARING_VIOLATION",
    "0x80070020",
    "0x800700B7",
];

pub fn is_transient_error(err_str: &str) -> bool {
    TRANSIENT_MARKERS.iter().any(|marker| err_str.contains(marker))
}

pub async fn retry_with_backoff<F, Fut, T>(
    operation_name: &str,
    max_retries: usize,
    base_delay_ms: u64,
    mut op: F,
) -> Result<T, EngineError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, EngineError>>,
{
    let mut attempt = 1;
    let mut delay = base_delay_ms;

    loop {
        match op().await {
            Ok(val) => return Ok(val),
            Err(err) => {
                let err_str = err.to_string();
                if is_transient_error(&err_str) && attempt <= max_retries {
                    eprintln!(
                        "rcompose: transient lock detected during '{}' ({}/{}). Retrying in {}ms...",
                        operation_name, attempt, max_retries, delay
                    );
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    attempt += 1;
                    delay = (delay as f64 * 1.5) as u64;
                } else {
                    return Err(err);
                }
            }
        }
    }
}
