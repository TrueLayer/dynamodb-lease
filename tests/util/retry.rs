use crate::util::TEST_WAIT;
use std::{
    fmt,
    future::Future,
    time::{Duration, Instant},
};

/// Calls the async function repeatedly until the result is `Ok(_)`.
///
/// * Pauses 50ms between tries.
/// * Will retry for up to [`TEST_WAIT`] before panicking.
pub async fn until_ok<F, O, T>(f: F) -> T
where
    F: Fn() -> O,
    O: Future<Output = anyhow::Result<T>>,
    T: fmt::Debug,
{
    let start = Instant::now();
    loop {
        let result = f().await;
        if let Ok(out) = result {
            return out;
        }
        assert!(start.elapsed() < TEST_WAIT, "{}", result.unwrap_err());
        tokio::time::sleep(Duration::from_millis(50)).await
    }
}
