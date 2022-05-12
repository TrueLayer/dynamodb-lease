#!/usr/bin/env bash
## Sets up dynamodb using docker (if ports are not already occupied).

set -eu

if ! nc -z localhost 8000; then
  echo 'starting docker dynamodb-local on port 8000' >&2
  docker run --rm -d -p 8000:8000 amazon/dynamodb-local
fi
