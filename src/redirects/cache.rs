//! In-memory TTL cache for upstream JSON documents.
//!
//! Wraps a [`moka::future::Cache`] with a single, fixed key per upstream so the
//! callers do not need to hold global state. The cache stores the *parsed*
//! upstream payload, not the raw bytes — parsing only happens on a cache miss.

use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;

/// Default time-to-live for cached upstream documents (6 hours).
///
/// Both upstreams are slow-moving (a handful of commits per week), so a long
/// TTL drastically reduces network chatter without hiding meaningful updates
/// from users.
pub const DEFAULT_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Single-key cache holding an `Arc<T>` payload.
///
/// Using `Arc<T>` keeps clones cheap: every lookup hands out a refcount bump
/// instead of cloning a potentially large parsed document.
#[derive(Clone)]
pub struct SingleEntryCache<T: Send + Sync + 'static> {
    inner: Cache<&'static str, Arc<T>>,
}

impl<T: Send + Sync + 'static> SingleEntryCache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Cache::builder().max_capacity(1).time_to_live(ttl).build(),
        }
    }

    /// Returns the cached value if present, otherwise computes it via `init`
    /// and inserts it. `init` is only awaited on a cache miss.
    pub async fn get_or_try_insert_with<F, Fut, E>(&self, init: F) -> Result<Arc<T>, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        if let Some(v) = self.inner.get(&"v").await {
            return Ok(v);
        }
        let value = Arc::new(init().await?);
        self.inner.insert("v", value.clone()).await;
        Ok(value)
    }

    /// Drop the cached value (next lookup will refetch).
    pub async fn invalidate(&self) {
        self.inner.invalidate(&"v").await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn caches_first_call() {
        let cache: SingleEntryCache<u32> = SingleEntryCache::new(Duration::from_secs(60));
        let calls = Arc::new(AtomicUsize::new(0));

        for _ in 0..5 {
            let calls = calls.clone();
            let v = cache
                .get_or_try_insert_with::<_, _, std::convert::Infallible>(|| async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(42)
                })
                .await
                .unwrap();
            assert_eq!(*v, 42);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn invalidate_forces_recompute() {
        let cache: SingleEntryCache<u32> = SingleEntryCache::new(Duration::from_secs(60));
        let calls = Arc::new(AtomicUsize::new(0));

        for _ in 0..2 {
            let calls = calls.clone();
            cache
                .get_or_try_insert_with::<_, _, std::convert::Infallible>(|| async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(1)
                })
                .await
                .unwrap();
            cache.invalidate().await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn does_not_cache_on_error() {
        let cache: SingleEntryCache<u32> = SingleEntryCache::new(Duration::from_secs(60));
        let calls = Arc::new(AtomicUsize::new(0));

        for _ in 0..3 {
            let calls = calls.clone();
            let res = cache
                .get_or_try_insert_with::<_, _, &'static str>(|| async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Err("boom")
                })
                .await;
            assert!(res.is_err());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}
