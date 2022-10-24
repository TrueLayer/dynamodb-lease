# dynamodb-lease
Client that acquires distributed locks in dynamodb that expire (aka "leases").
Uses [aws-sdk-dynamodb](https://github.com/awslabs/aws-sdk-rust/tree/main/sdk/dynamodb)
& [tokio](https://github.com/tokio-rs/tokio).

[![Crates.io](https://img.shields.io/crates/v/dynamodb-lease.svg)](https://crates.io/crates/dynamodb-lease)
[![Docs.rs](https://docs.rs/dynamodb-lease/badge.svg)](https://docs.rs/dynamodb-lease)

```rust
let client = dynamodb_lease::Client::builder()
    .table_name("example-leases")
    .lease_ttl_seconds(60)
    .build_and_check_db(dynamodb_client)
    .await?;

// acquire a lease for "important-job-123"
// waits for any other holders to release if necessary
let lease = client.acquire("important-job-123").await?;
 
// `lease` periodically extends itself in a background tokio task
 
// until dropped others will not be able to acquire this lease
assert!(client.try_acquire("important-job-123").await?.is_none());

// Dropping the lease will asynchronously release it, so others may acquire it
drop(lease);
```

See the [design doc](./DESIGN.md) & source for how it works under the hood.

## Test
Run `scripts/init-test.sh` to ensure dynamodb-local is running on 8000.

```sh
cargo test
```

### AWS setup
You may also need to setup some aws config, e.g.
- setup `~/.aws/config` 
    ```
    [default]
    region = eu-west-1
    ```
- setup `~/.aws/credentials` with fakes values
    ```
    [default]
    aws_access_key_id=access_key
    aws_secret_access_key=secret_access_key
    ```
