use crate::Client;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedMutexGuard};
use uuid::Uuid;

/// Represents a held distributed lease & background task to
/// continuously try to extend it until dropped.
///
/// On drop asynchronously releases the underlying lock.
#[derive(Debug)]
pub struct Lease {
    client: Client,
    key_lease_v: Arc<(String, Mutex<Uuid>)>,
    /// A local guard to avoid db contention for leases within the same client.
    local_guard: Option<OwnedMutexGuard<()>>,
}

impl Lease {
    pub(crate) fn new(client: Client, key: String, lease_v: Uuid) -> Self {
        let lease = Self {
            client,
            key_lease_v: Arc::new((key, Mutex::new(lease_v))),
            local_guard: None,
        };

        start_periodicly_extending(&lease);

        lease
    }

    pub(crate) fn with_local_guard(mut self, guard: OwnedMutexGuard<()>) -> Self {
        self.local_guard = Some(guard);
        self
    }
}

fn start_periodicly_extending(lease: &Lease) {
    let key_lease_v = Arc::downgrade(&lease.key_lease_v);
    let client = lease.client.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(client.extend_period).await;
            match key_lease_v.upgrade() {
                Some(key_lease_v) => {
                    let mut lease_v = key_lease_v.1.lock().await;
                    let key = key_lease_v.0.clone();
                    match client.extend_lease(key, *lease_v).await {
                        Ok(new_lease_v) => *lease_v = new_lease_v,
                        // stop on error, TODO retries, logs?
                        Err(_) => break,
                    }
                }
                // lease dropped
                None => break,
            }
        }
    });
}

impl Drop for Lease {
    /// Asynchronously releases the underlying lock.
    fn drop(&mut self) {
        let client = self.client.clone();
        let key_lease_v = self.key_lease_v.clone();

        // Drop local guard *before* deleting lease to avoid unfair local acquire advantage.
        // Dropping the local_guard after deleting would be more efficient however during 
        // contention that efficiency could starve remote attempts to acquire the lease.
        drop(self.local_guard.take());
        client.try_clean_local_lock(key_lease_v.0.clone());

        tokio::spawn(async move {
            let lease_v = key_lease_v.1.lock().await;
            let key = key_lease_v.0.clone();
            // TODO retries, logs?
            let _ = client.delete_lease(key, *lease_v).await;
        });
    }
}
