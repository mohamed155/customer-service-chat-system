# Developer entry points for the local services (PostgreSQL + pgvector, Redis, MinIO, Mailpit).
# Install `just`: https://github.com/casey/just (brew install just, winget install Casey.Just,
# cargo install just --locked, or your distro's package). Run `just` to list the recipes.
#
# Recipes call bash. On Windows run them from WSL2 or Git Bash, not from PowerShell or cmd.
# The recipes only wrap Docker Compose and one Node script; they hold no logic of their own
# (ADR-015). The stack is defined in infra/docker-compose.yml, configured by .env.

set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments

# Own project name, so the volumes are called customer-service-dev_* and `just reset` can only
# ever touch this stack. Without it Compose would use the folder name ("infra") and, with
# `down -v`, would also delete the volumes of the observability profile and of other projects
# started from a folder with the same name. Exported so Compose and the doctor script see it.
export COMPOSE_PROJECT_NAME := "customer-service-dev"

compose := "docker compose --env-file .env -f infra/docker-compose.yml"

# List the recipes.
default:
    @just --list

# Create .env from .env.example the first time. Never overwrites an existing .env.
[private]
env:
    @if [ ! -f .env ]; then cp .env.example .env; echo "created .env from .env.example (dev-only values)"; fi

# Start the services and wait until every one reports healthy.
up: env
    {{compose}} up -d --wait

# Stop the services. Data stays in the named volumes.
down: env
    {{compose}} down

# Delete ALL local data (Postgres, Redis, MinIO, Mailpit volumes) and start from scratch.
[confirm("This deletes all local Postgres, Redis, MinIO and Mailpit data. Continue?")]
reset: env
    {{compose}} down -v
    {{compose}} up -d --wait

# Follow the logs of all services, or of the ones you name: just logs postgres minio
logs *services: env
    {{compose}} logs -f --tail=100 "$@"

# Open psql on the local database, or run a command: just psql -c "select version()"
psql *args: env
    {{compose}} exec $([ -t 0 ] || echo -T) postgres sh -c 'exec psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" "$@"' sh "$@"

# Prove the app_runtime role cannot bypass Row-Level Security (needs the stack running).
verify-roles: env
    {{compose}} exec -T postgres bash -s < infra/postgres/verify-roles.sh

# Check Docker, Compose, Node, .env and the host ports before you start.
doctor:
    node infra/doctor.mjs
