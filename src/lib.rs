//! Client that acquires distributed locks with an expiry (aka "leases") using dynamodb & tokio runtime.
//!
//! # Example
//! ```
//! # use std::time::Duration;
//! # async fn foo() -> anyhow::Result<()> {
//! # let dynamodb_client: aws_sdk_dynamodb::Client = unimplemented!();
//! let client = dynamodb_lease::Client::builder()
//!     .table_name("example-leases")
//!     .lease_ttl_seconds(60)
//!     .build_and_check_db(dynamodb_client)
//!     .await?;
//!
//! // acquire a lease for "important-job-123"
//! // waits for any other holders to release if necessary
//! let lease = client.acquire("important-job-123").await?;
//!
//! // `lease` periodically extends itself in a background tokio task
//!
//! // until dropped others will not be able to acquire this lease
//! assert!(client.try_acquire("important-job-123").await?.is_none());
//!
//! // Dropping the lease will asynchronously release it, so others may acquire it
//! drop(lease);
//! # Ok(()) }
//! ```

mod builder;
mod client;
mod lease;

pub use builder::ClientBuilder;
pub use client::Client;
pub use lease::Lease;
