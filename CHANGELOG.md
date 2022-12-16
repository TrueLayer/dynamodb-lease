# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.10.0
* Update _aws-sdk-dynamodb_ to `0.22`.

## 0.9.0
* Update _aws-sdk-dynamodb_ to `0.21`.

## 0.8.0
* Update _aws-sdk-dynamodb_ to `0.19`.

## 0.7.0
* Update _aws-sdk-dynamodb_ to `0.18`.

## 0.6.0
* Update _aws-sdk-dynamodb_ to `0.17`.

## 0.5.0
* Update _aws-sdk-dynamodb_ to `0.16`.

## 0.4.1
* Add _tracing_ support.
* Tweak acquisition logic to remove an unfair advantage if just dropped a lease that could
  starve remote acquisition attempts under high contention.

## 0.4.0
* Update _aws-sdk-dynamodb_ to `0.15`.

## 0.3.0
* Update _aws-sdk-dynamodb_ to `0.14`.

## 0.2.0
* Update _aws-sdk-dynamodb_ to `0.13`.

## 0.1.0
* Add `dynamodb_lease::Client` & friends.
