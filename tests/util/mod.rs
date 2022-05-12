pub mod retry;

use aws_sdk_dynamodb::{
    error::{CreateTableError, CreateTableErrorKind},
    model::{
        AttributeDefinition, BillingMode, KeySchemaElement, KeyType, ScalarAttributeType,
        TimeToLiveSpecification,
    },
    types::SdkError,
};
use std::time::Duration;

/// Test wait timeout, generally long enough that something has probably gone wrong.
pub const TEST_WAIT: Duration = Duration::from_secs(4);

/// Config for localhost dynamodb.
pub async fn localhost_dynamodb() -> aws_sdk_dynamodb::Client {
    let conf = aws_config::from_env().region("eu-west-1").load().await;
    let conf = aws_sdk_dynamodb::config::Builder::from(&conf)
        .endpoint_resolver(aws_sdk_dynamodb::Endpoint::immutable(
            "http://localhost:8000".parse().unwrap(),
        ))
        .build();
    aws_sdk_dynamodb::Client::from_conf(conf)
}

/// Create the table, with "key" as a hash key, if it doesn't exist.
pub async fn create_lease_table(table_name: &str, client: &aws_sdk_dynamodb::Client) {
    let create_table = client
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

    match create_table {
        Ok(_)
        | Err(SdkError::ServiceError {
            err:
                CreateTableError {
                    kind: CreateTableErrorKind::ResourceInUseException(..),
                    ..
                },
            ..
        }) => Ok(()),
        Err(e) => Err(e),
    }
    .expect("dynamodb create_table failed: Did you run scripts/init-test.sh ?");

    let ttl_update = client
        .update_time_to_live()
        .table_name(table_name)
        .time_to_live_specification(
            TimeToLiveSpecification::builder()
                .enabled(true)
                .attribute_name("lease_expiry")
                .build(),
        )
        .send()
        .await;
    match ttl_update {
        Ok(_) => Ok(()),
        Err(SdkError::ServiceError { err, .. })
            if err.code() == Some("ValidationException")
                && err.message() == Some("TimeToLive is already enabled") =>
        {
            Ok(())
        }

        Err(e) => Err(e),
    }
    .expect("dynamodb ttl_update failed");
}
