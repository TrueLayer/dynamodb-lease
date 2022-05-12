mod util;

use anyhow::Context;
use aws_sdk_dynamodb::model::{
    AttributeDefinition, BillingMode, KeySchemaElement, KeyType, ScalarAttributeType,
};
use std::time::Duration;
use util::*;
use uuid::Uuid;

#[tokio::test]
async fn try_acquire() {
    let lease_table = "test-locker-leases";
    let db_client = localhost_dynamodb().await;
    create_lease_table(lease_table, &db_client).await;

    let client = dynamodb_lease::Client::builder()
        .table_name(lease_table)
        .build_and_check_db(db_client)
        .await
        .unwrap();

    let lease_key = format!("try_acquire:{}", Uuid::new_v4());

    // acquiring a lease should work
    let lease1 = client.try_acquire(&lease_key).await.unwrap();
    assert!(lease1.is_some());

    // subsequent attempts should fail
    let lease2 = client.try_acquire(&lease_key).await.unwrap();
    assert!(lease2.is_none());
    let lease2 = client.try_acquire(&lease_key).await.unwrap();
    assert!(lease2.is_none());

    // dropping should asynchronously end the lease
    drop(lease1);

    // in shortish order the key should be acquirable again
    retry::until_ok(|| async {
        client
            .try_acquire(&lease_key)
            .await
            .and_then(|maybe_lease| maybe_lease.context("did not acquire"))
    })
    .await;
}

#[tokio::test]
#[ignore = "slow"]
async fn try_acquire_extend_past_ttl() {
    let lease_table = "test-locker-leases";
    let db_client = localhost_dynamodb().await;
    create_lease_table(lease_table, &db_client).await;

    let client = dynamodb_lease::Client::builder()
        .table_name(lease_table)
        .lease_ttl_seconds(2)
        .build_and_check_db(db_client)
        .await
        .unwrap();

    let lease_key = format!("try_acquire_extend_past_expiry:{}", Uuid::new_v4());

    // acquiring a lease should work
    let lease1 = client.try_acquire(&lease_key).await.unwrap();
    assert!(lease1.is_some());

    // subsequent attempts should fail
    assert!(client.try_acquire(&lease_key).await.unwrap().is_none());

    // after some time the original lease will have expired
    // however, a background task should have extended it so it should still be active.
    // Note: Need to wait ages to reliably trigger ttl deletion :(
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(
        client.try_acquire(&lease_key).await.unwrap().is_none(),
        "lease should have been extended"
    );
}

#[tokio::test]
async fn acquire() {
    let lease_table = "test-locker-leases";
    let db_client = localhost_dynamodb().await;
    create_lease_table(lease_table, &db_client).await;

    let client = dynamodb_lease::Client::builder()
        .table_name(lease_table)
        .build_and_check_db(db_client)
        .await
        .unwrap();

    let lease_key = format!("acquire:{}", Uuid::new_v4());

    // acquiring a lease should work
    let lease1 = client.acquire(&lease_key).await.unwrap();

    // subsequent attempts should fail
    let lease2 = tokio::time::timeout(Duration::from_millis(100), client.acquire(&lease_key)).await;
    assert!(lease2.is_err(), "should not acquire while lease1 is alive");

    // dropping should asynchronously end the lease
    drop(lease1);

    // in shortish order the key should be acquirable again
    tokio::time::timeout(TEST_WAIT, client.acquire(&lease_key))
        .await
        .expect("could not acquire after drop")
        .expect("failed to acquire");
}

#[tokio::test]
async fn init_should_check_table_exists() {
    let db_client = localhost_dynamodb().await;

    let err = dynamodb_lease::Client::builder()
        .table_name("test-locker-leases-not-exists")
        .build_and_check_db(db_client)
        .await
        .expect_err("should check table exists");
    assert!(
        err.to_string().to_ascii_lowercase().contains("missing"),
        "{}",
        err
    );
}

#[tokio::test]
async fn init_should_check_hash_key() {
    let table_name = "table-with-wrong-key";
    let db_client = localhost_dynamodb().await;

    let _ = db_client
        .create_table()
        .table_name(table_name)
        .billing_mode(BillingMode::PayPerRequest)
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("wrong")
                .attribute_type(ScalarAttributeType::S)
                .build(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("wrong")
                .key_type(KeyType::Hash)
                .build(),
        )
        .send()
        .await;

    let err = dynamodb_lease::Client::builder()
        .table_name(table_name)
        .build_and_check_db(db_client)
        .await
        .expect_err("should check hash 'key'");
    assert!(
        err.to_string().to_ascii_lowercase().contains("key"),
        "{}",
        err
    );
}

#[tokio::test]
async fn init_should_check_hash_key_type() {
    let table_name = "table-with-wrong-key-type";
    let db_client = localhost_dynamodb().await;

    let _ = db_client
        .create_table()
        .table_name(table_name)
        .billing_mode(BillingMode::PayPerRequest)
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("key")
                .attribute_type(ScalarAttributeType::N)
                .build(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("key")
                .key_type(KeyType::Hash)
                .build(),
        )
        .send()
        .await;

    let err = dynamodb_lease::Client::builder()
        .table_name(table_name)
        .build_and_check_db(db_client)
        .await
        .expect_err("should check hash key type");
    assert!(
        err.to_string().to_ascii_lowercase().contains("type"),
        "{}",
        err
    );
}

#[tokio::test]
async fn init_should_check_ttl() {
    let table_name = "table-with-without-ttl";
    let db_client = localhost_dynamodb().await;

    let _ = db_client
        .create_table()
        .table_name(table_name)
        .billing_mode(BillingMode::PayPerRequest)
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("key")
                .attribute_type(ScalarAttributeType::S)
                .build(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("key")
                .key_type(KeyType::Hash)
                .build(),
        )
        .send()
        .await;

    let err = dynamodb_lease::Client::builder()
        .table_name(table_name)
        .build_and_check_db(db_client)
        .await
        .expect_err("should check ttl");
    assert!(
        err.to_string()
            .to_ascii_lowercase()
            .contains("time to live"),
        "{}",
        err
    );
}
