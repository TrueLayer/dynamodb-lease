use crate::{local::LocalLocks, ClientBuilder, Lease};
use anyhow::{bail, ensure, Context};
use aws_sdk_dynamodb::{
    error::{DeleteItemError, PutItemError, PutItemErrorKind, UpdateItemError},
    model::{AttributeValue, KeyType, ScalarAttributeType},
    output::DeleteItemOutput,
    types::SdkError,
};
use std::{
    cmp::min,
    sync::Arc,
    time::{Duration, Instant},
};
use time::OffsetDateTime;
use tracing::instrument;
use uuid::Uuid;

const KEY_FIELD: &str = "key";
const LEASE_EXPIRY_FIELD: &str = "lease_expiry";
const LEASE_VERSION_FIELD: &str = "lease_version";

/// Client for acquiring [`Lease`]s.
///
/// Communicates with dynamodb to acquire, extend and delete distributed leases.
///
/// Local mutex locks are also used to eliminate db contention for usage within
/// a single `Client` instance or clone.
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) client: aws_sdk_dynamodb::Client,
    pub(crate) table_name: Arc<String>,
    pub(crate) lease_ttl_seconds: u32,
    pub(crate) extend_period: Duration,
    pub(crate) acquire_cooldown: Duration,
    pub(crate) local_locks: LocalLocks,
}

impl Client {
    /// Returns a new [`Client`] builder.
    pub fn builder() -> ClientBuilder {
        <_>::default()
    }

    /// Trys to acquire a new [`Lease`] for the given `key`.
    ///
    /// If this lease has already been acquired elsewhere `Ok(None)` is returned.
    ///
    /// Does not wait to acquire a lease, to do so see [`Client::acquire`].
    #[instrument(skip_all)]
    pub async fn try_acquire(&self, key: impl Into<String>) -> anyhow::Result<Option<Lease>> {
        let key = key.into();
        let local_guard = match self.local_locks.try_lock(key.clone()) {
            Ok(g) => g,
            Err(_) => return Ok(None),
        };

        match self.put_lease(key).await {
            Ok(Some(lease)) => Ok(Some(lease.with_local_guard(local_guard))),
            x => x,
        }
    }

    /// Acquires a new [`Lease`] for the given `key`. May wait until successful if the lease
    /// has already been acquired elsewhere.
    ///
    /// To try to acquire without waiting see [`Client::try_acquire`].
    #[instrument(skip_all)]
    pub async fn acquire(&self, key: impl Into<String>) -> anyhow::Result<Lease> {
        let key = key.into();
        let local_guard = self.local_locks.lock(key.clone()).await;

        loop {
            if let Some(lease) = self.put_lease(key.clone()).await? {
                return Ok(lease.with_local_guard(local_guard));
            }
            tokio::time::sleep(self.acquire_cooldown).await;
        }
    }

    /// Acquires a new [`Lease`] for the given `key`. May wait until successful if the lease
    /// has already been acquired elsewhere up to a max of `max_wait`.
    ///
    /// To try to acquire without waiting see [`Client::try_acquire`].
    #[instrument(skip_all)]
    pub async fn acquire_timeout(
        &self,
        key: impl Into<String>,
        max_wait: Duration,
    ) -> anyhow::Result<Lease> {
        let start = Instant::now();
        let key = key.into();

        let local_guard = tokio::time::timeout(max_wait, self.local_locks.lock(key.clone()))
            .await
            .context("Could not acquire within {max_wait:?}")?;

        loop {
            if let Some(lease) = self.put_lease(key.clone()).await? {
                return Ok(lease.with_local_guard(local_guard));
            }
            let elapsed = start.elapsed();
            if elapsed > max_wait {
                bail!("Could not acquire within {max_wait:?}");
            }
            let remaining_max_wait = max_wait - elapsed;
            tokio::time::sleep(min(self.acquire_cooldown, remaining_max_wait)).await;
        }
    }

    /// Put a new lease into the db.
    async fn put_lease(&self, key: String) -> anyhow::Result<Option<Lease>> {
        let expiry_timestamp =
            OffsetDateTime::now_utc().unix_timestamp() + i64::from(self.lease_ttl_seconds);
        let lease_v = Uuid::new_v4();

        let put = self
            .client
            .put_item()
            .table_name(self.table_name.as_str())
            .item(KEY_FIELD, AttributeValue::S(key.clone()))
            .item(
                LEASE_EXPIRY_FIELD,
                AttributeValue::N(expiry_timestamp.to_string()),
            )
            .item(LEASE_VERSION_FIELD, AttributeValue::S(lease_v.to_string()))
            .condition_expression(format!("attribute_not_exists({LEASE_VERSION_FIELD})"))
            .send()
            .await;

        match put {
            Err(SdkError::ServiceError {
                err:
                    PutItemError {
                        kind: PutItemErrorKind::ConditionalCheckFailedException(..),
                        ..
                    },
                ..
            }) => Ok(None),
            Err(err) => Err(err.into()),
            Ok(_) => Ok(Some(Lease::new(self.clone(), key, lease_v))),
        }
    }

    /// Delete a lease with a given `key` & `lease_v`.
    #[instrument(skip_all)]
    pub(crate) async fn delete_lease(
        &self,
        key: String,
        lease_v: Uuid,
    ) -> Result<DeleteItemOutput, SdkError<DeleteItemError>> {
        self.client
            .delete_item()
            .table_name(self.table_name.as_str())
            .key(KEY_FIELD, AttributeValue::S(key))
            .condition_expression(format!("{LEASE_VERSION_FIELD}=:lease_v"))
            .expression_attribute_values(":lease_v", AttributeValue::S(lease_v.to_string()))
            .send()
            .await
    }

    /// Cleanup local lock memory for the given `key` if not in use.
    pub(crate) fn try_clean_local_lock(&self, key: String) {
        self.local_locks.try_remove(key)
    }

    /// Extends an active lease. Returns the new `lease_v` uuid.
    #[instrument(skip_all)]
    pub(crate) async fn extend_lease(
        &self,
        key: String,
        lease_v: Uuid,
    ) -> Result<Uuid, SdkError<UpdateItemError>> {
        let expiry_timestamp =
            OffsetDateTime::now_utc().unix_timestamp() + i64::from(self.lease_ttl_seconds);
        let new_lease_v = Uuid::new_v4();

        self.client
            .update_item()
            .table_name(self.table_name.as_str())
            .key(KEY_FIELD, AttributeValue::S(key))
            .update_expression(format!(
                "SET {LEASE_VERSION_FIELD}=:new_lease_v, {LEASE_EXPIRY_FIELD}=:expiry"
            ))
            .condition_expression(format!("{LEASE_VERSION_FIELD}=:lease_v"))
            .expression_attribute_values(":new_lease_v", AttributeValue::S(new_lease_v.to_string()))
            .expression_attribute_values(":lease_v", AttributeValue::S(lease_v.to_string()))
            .expression_attribute_values(":expiry", AttributeValue::N(expiry_timestamp.to_string()))
            .send()
            .await?;

        Ok(new_lease_v)
    }

    /// Checks table is active & has a valid schema.
    pub(crate) async fn check_schema(&self) -> anyhow::Result<()> {
        // fetch table & ttl descriptions concurrently
        let (table_desc, ttl_desc) = tokio::join!(
            self.client
                .describe_table()
                .table_name(self.table_name.as_str())
                .send(),
            self.client
                .describe_time_to_live()
                .table_name(self.table_name.as_str())
                .send()
        );

        let desc = table_desc
            .with_context(|| format!("Missing table `{}`?", self.table_name))?
            .table
            .context("no table description")?;

        // check "key" field is a S hash key
        let attrs = desc.attribute_definitions.unwrap_or_default();
        let key_schema = desc.key_schema.unwrap_or_default();
        ensure!(
            key_schema.len() == 1,
            "Unexpected number of keys ({}) in key_schema, expected 1. Got {:?}",
            key_schema.len(),
            vec(key_schema.iter().map(|k| k.attribute_name().unwrap_or("?"))),
        );
        let described_kind = attrs
            .iter()
            .find(|attr| attr.attribute_name() == Some(KEY_FIELD))
            .with_context(|| {
                format!(
                    "Missing attribute definition for {KEY_FIELD}, available {:?}",
                    vec(attrs.iter().filter_map(|a| a.attribute_name()))
                )
            })?
            .attribute_type()
            .with_context(|| format!("Missing attribute type for {KEY_FIELD}"))?;
        ensure!(
            described_kind == &ScalarAttributeType::S,
            "Unexpected attribute type `{:?}` for {}, expected `{:?}`",
            described_kind,
            KEY_FIELD,
            ScalarAttributeType::S,
        );

        let described_key_type = key_schema
            .iter()
            .find(|k| k.attribute_name() == Some(KEY_FIELD))
            .with_context(|| {
                format!(
                    "Missing key schema for {KEY_FIELD}, available {:?}",
                    vec(key_schema.iter().filter_map(|k| k.attribute_name()))
                )
            })?
            .key_type()
            .with_context(|| format!("Missing key type for {KEY_FIELD}"))?;
        ensure!(
            described_key_type == &KeyType::Hash,
            "Unexpected key type `{:?}` for {}, expected `{:?}`",
            described_key_type,
            KEY_FIELD,
            KeyType::Hash,
        );

        // check "lease_expiry" is a ttl field
        let update_time_to_live_desc = ttl_desc
            .with_context(|| format!("Missing time_to_live for table `{}`?", self.table_name))?
            .time_to_live_description
            .context("no time to live description")?;

        ensure!(
            update_time_to_live_desc.attribute_name() == Some(LEASE_EXPIRY_FIELD),
            "time to live for {} is not set",
            LEASE_EXPIRY_FIELD,
        );

        Ok(())
    }
}

#[inline]
fn vec<T>(iter: impl Iterator<Item = T>) -> Vec<T> {
    iter.collect()
}
