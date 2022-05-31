use crate::Client;
use std::time::Duration;

/// [`Client`] builder.
pub struct ClientBuilder {
    table_name: String,
    lease_ttl_seconds: u32,
    extend_period: Option<Duration>,
    acquire_cooldown: Duration,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            table_name: "leases".into(),
            lease_ttl_seconds: 60,
            extend_period: None,
            acquire_cooldown: Duration::from_secs(1),
        }
    }
}

impl ClientBuilder {
    /// Sets the lease table name where the lease info will be stored.
    /// The table must have the correct schema.
    ///
    /// Default `"leases"`.
    pub fn table_name(mut self, table_name: impl Into<String>) -> Self {
        self.table_name = table_name.into();
        self
    }

    /// Sets the time to live for each lease (or lease extension) in seconds.
    /// **Must be at least 2**.
    ///
    /// Note: Time to live is implemented using the native dynamodb feature. As this
    /// is based on unix timestamps the unit is seconds. This makes extending ttls lower
    /// than 2s not reliable.
    ///
    /// Note: A [`crate::Lease`] will attempt to extend itself in the background until dropped
    /// and then release itself. However, since db comms failing after acquiring a lease
    /// is possible, this ttl is the _guaranteed_ lifetime of a lease. As such, and since
    /// ttl is not in normal operation relied upon to release leases, it may make sense
    /// to set this higher than the max operation time the lease is wrapping.
    ///
    /// So, for example, if the locked task can take 1s to 5m a ttl of 10m should provide
    /// a decent guarantee that such tasks will never execute concurrently. In normal operation
    /// each lease will release (be deleted) immediately after dropping, so having a high
    /// ttl only affects the edge case where the extend/drop db interactions fail.
    ///
    /// Default `60`.
    ///
    /// # Panics
    /// Panics if less than 2s.
    pub fn lease_ttl_seconds(mut self, seconds: u32) -> Self {
        assert!(
            seconds >= 2,
            "must be at least 2s, shorter ttls are not supported"
        );
        self.lease_ttl_seconds = seconds;
        self
    }

    /// Sets the periodic duration between each background attempt to extend the lease. These
    /// happen continually while the [`crate::Lease`] is alive.
    ///
    /// Each extension renews the lease to the full ttl. This duration must be less
    /// than the ttl.
    ///
    /// Default `lease_ttl_seconds / 2`.
    ///
    /// # Panics
    /// Panics if zero.
    pub fn extend_every(mut self, extend_period: Duration) -> Self {
        assert!(extend_period > Duration::ZERO, "must be greater than zero");
        self.extend_period = Some(extend_period);
        self
    }

    /// Sets how long [`Client::acquire`] waits between attempts to acquire a lease.
    ///
    /// Default `1s`.
    pub fn acquire_cooldown(mut self, cooldown: Duration) -> Self {
        self.acquire_cooldown = cooldown;
        self
    }

    /// Builds a [`Client`] and checks the dynamodb table is active with the correct schema.
    ///
    /// # Panics
    /// Panics if `extend_period` is not less than `lease_ttl_seconds`.
    pub async fn build_and_check_db(
        self,
        dynamodb_client: aws_sdk_dynamodb::Client,
    ) -> anyhow::Result<Client> {
        let extend_period = self
            .extend_period
            .unwrap_or_else(|| Duration::from_secs_f64(self.lease_ttl_seconds as f64 / 2.0));
        assert!(
            extend_period < Duration::from_secs(self.lease_ttl_seconds as _),
            "renew_period must be less than ttl"
        );

        let client = Client {
            table_name: self.table_name.into(),
            client: dynamodb_client,
            lease_ttl_seconds: self.lease_ttl_seconds,
            extend_period,
            acquire_cooldown: self.acquire_cooldown,
            local_locks: <_>::default(),
        };

        client.check_schema().await?;

        Ok(client)
    }
}
