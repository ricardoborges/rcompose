//! Signal handling for graceful termination.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn setup_ctrl_c() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("\nGracefully shutting down...");
            r.store(false, Ordering::SeqCst);
        }
    });

    running
}
