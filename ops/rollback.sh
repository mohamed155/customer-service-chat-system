#!/usr/bin/env bash
# =============================================================================
# ops/rollback.sh — Roll back to a previous release
# =============================================================================
# Contract: specs/033-production-hardening/contracts/deployment-scripts.md
#
# Usage: ops/rollback.sh <previous-git-sha> [--force]
#
# Returns the previous release to service. Same steps as deploy minus
# migrations.
#
# Refuses by default when migrations ran between the two SHAs (FR-024).
# --force overrides for the operator who has confirmed the migrations are
# backward-compatible.
#
# Exit codes: 0 success, 1 failure, 2 misuse
# =============================================================================

set -euo pipefail

readonly PROG="$(basename "$0")"

# ---------------------------------------------------------------------------
# Usage and argument parsing
# ---------------------------------------------------------------------------
usage() {
    echo "Usage: $PROG <previous-git-sha> [--force]" >&2
    exit 2
}

FORCE=false
DRY_RUN=false
POSITIONAL=()

for arg in "$@"; do
    case "$arg" in
        --force) FORCE=true ;;
        --dry-run) DRY_RUN=true ;;
        --*) echo "ERROR: unknown option: $arg" >&2; usage ;;
        *) POSITIONAL+=("$arg") ;;
    esac
done

PREVIOUS_SHA="${POSITIONAL[0]:-}"
[[ -n "$PREVIOUS_SHA" ]] || usage

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
readonly COMPOSE_FILES="--profile observability -f infra/docker-compose.yml -f infra/docker-compose.staging.yml"
readonly IMAGE_BASE="${IMAGE_BASE:-ghcr.io/mohamed155/customer-service-chat-system}"
readonly BACKEND_IMAGE="${IMAGE_BASE}-backend:${PREVIOUS_SHA}"
readonly DASHBOARD_IMAGE="${IMAGE_BASE}-dashboard:${PREVIOUS_SHA}"
readonly WIDGET_IMAGE="${IMAGE_BASE}-widget:${PREVIOUS_SHA}"

# ---------------------------------------------------------------------------
# Dry-run helper
# ---------------------------------------------------------------------------
dry_run() {
    echo "[DRY-RUN] $*"
}

run_or_dry() {
    if $DRY_RUN; then
        dry_run "$@"
    else
        "$@"
    fi
}

# ---------------------------------------------------------------------------
# Step 0: Determine current SHA
# ---------------------------------------------------------------------------
CURRENT_SHA="${APP_RELEASE_SHA:-}"
if [[ -z "$CURRENT_SHA" ]]; then
    if ! $DRY_RUN; then
        CURRENT_SHA="$(
            docker compose $COMPOSE_FILES exec -T backend \
                sh -c 'echo "${APP_RELEASE_SHA:-}"' 2>/dev/null || true
        )"
    fi
fi

echo "=== Rolling back from ${CURRENT_SHA:-unknown} to $PREVIOUS_SHA ==="
[[ -n "$CURRENT_SHA" ]] || echo "WARNING: Could not determine current SHA"

# ---------------------------------------------------------------------------
# Step 1: Verify the target SHA exists
# ---------------------------------------------------------------------------
echo "--- Step 1/4: Verifying SHA ---"

if $DRY_RUN; then
    dry_run "git rev-parse --verify $PREVIOUS_SHA"
else
    if ! git rev-parse --verify "$PREVIOUS_SHA^{commit}" >/dev/null 2>&1; then
        echo "ERROR: Git SHA '$PREVIOUS_SHA' not found in local repository" >&2
        exit 1
    fi
fi

# ---------------------------------------------------------------------------
# Step 2: Check for migrations between the two SHAs
# ---------------------------------------------------------------------------
echo "--- Step 2/4: Checking for migrations ---"

if [[ -n "$CURRENT_SHA" ]] && ! $DRY_RUN; then
    # Find migration files that differ between the two SHAs
    MIGRATIONS=$(git diff --name-only "$PREVIOUS_SHA" "$CURRENT_SHA" -- backend/migrations/ 2>/dev/null || true)

    if [[ -n "$MIGRATIONS" ]]; then
        echo "WARNING: Migrations were added between $PREVIOUS_SHA and $CURRENT_SHA:"
        echo "$MIGRATIONS" | while IFS= read -r m; do echo "  - $m"; done
        echo ""
        echo "Rolling back past these migrations may cause data loss or"
        echo "schema incompatibility. See docs/backup-restore.md for guidance"
        echo "on destructive migrations."

        if ! $FORCE; then
            echo ""
            echo "Use --force to override if you have confirmed these"
            echo "migrations are backward-compatible."
            exit 1
        fi

        echo "  --force set — proceeding despite migrations"
    else
        echo "  No migrations found between the two SHAs"
    fi
elif [[ -z "$CURRENT_SHA" ]]; then
    echo "  (cannot check migrations — current SHA unknown)"
fi

# ---------------------------------------------------------------------------
# Step 3: Pull old images
# ---------------------------------------------------------------------------
echo "--- Step 3/4: Pulling images ---"

run_or_dry docker pull "$BACKEND_IMAGE"
run_or_dry docker pull "$DASHBOARD_IMAGE"
run_or_dry docker pull "$WIDGET_IMAGE"

# ---------------------------------------------------------------------------
# Step 4: Deploy the previous release
# ---------------------------------------------------------------------------
echo "--- Step 4/4: Deploying previous release ---"

export APP_RELEASE_SHA="$PREVIOUS_SHA"

run_or_dry docker compose $COMPOSE_FILES up -d

if ! $DRY_RUN; then
    # Poll /ready
    TIMEOUT=120
    INTERVAL=5
    ELAPSED=0
    READY=false

    while [[ $ELAPSED -lt $TIMEOUT ]]; do
        if curl -sf http://localhost:8080/ready >/dev/null 2>&1; then
            READY=true
            echo "  Backend ready after ${ELAPSED}s"
            break
        fi
        sleep "$INTERVAL"
        ELAPSED=$((ELAPSED + INTERVAL))
    done

    if ! $READY; then
        echo "ERROR: Backend not ready within ${TIMEOUT}s after rollback." >&2
        exit 1
    fi

    # Reload Caddy
    if docker compose $COMPOSE_FILES ps -q caddy >/dev/null 2>&1; then
        docker compose $COMPOSE_FILES exec caddy caddy reload --config /etc/caddy/Caddyfile 2>/dev/null || true
    fi
fi

echo ""
echo "=== Rollback to $PREVIOUS_SHA completed successfully ==="
exit 0
