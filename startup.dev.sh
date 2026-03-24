#!/bin/bash

set -euo pipefail

docker compose --project-directory . -f ./compose.dev.yml up --build --force-recreate "$@"
