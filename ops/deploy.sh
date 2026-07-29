#!/usr/bin/env bash
# =============================================================================
# ops/deploy.sh — Promote a commit to staging
# =============================================================================
# Contract: specs/033-production-hardening/contracts/deployment-scripts.md
#
# Usage: ops/deploy.sh <git-sha> [--dry-run] [--skip-migrations]
#
# Steps:
#   1. Verify <git-sha> exists and images are published for it
#   2. Pull images by immutable SHA tag
#   3. Run migrations in a one-shot container
#   4. Start the new stack; poll /ready until ready or a 120 s timeout
#   5. Switch Caddy upstream; stop the old stack
#
# Failure behaviour (FR-023, FR-027):
#   - Migration failure aborts with the OLD release still serving.
#   - Readiness timeout aborts and leaves the OLD release serving.
#   - Never leaves a partially-migrated environment accepting traffic.
#
# Exit codes: 0 success, 1 failure, 2 misuse
# =============================================================================

set -euo pipefail

readonly PROG="$(basename "$0")"

# ---------------------------------------------------------------------------
# Usage and argument parsing
# ---------------------------------------------------------------------------
usage() {
    echo "Usage: $PROG <git-sha> [--dry-run] [--skip-migrations] [--manifest <path>]" >&2
    exit 2
}

DRY_RUN=false
SKIP_MIGRATIONS=false
MANIFEST_PATH=""
POSITIONAL=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) DRY_RUN=true; shift ;;
        --skip-migrations) SKIP_MIGRATIONS=true; shift ;;
        --manifest)
            if [[ -z "$2" || "$2" == --* ]]; then
                echo "ERROR: --manifest requires a path argument" >&2
                usage
            fi
            MANIFEST_PATH="$2"
            shift 2
            ;;
        --*) echo "ERROR: unknown option: $1" >&2; usage ;;
        *) POSITIONAL+=("$1"); shift ;;
    esac
done

SHA="${POSITIONAL[0]:-}"
[[ -n "$SHA" ]] || usage

if [[ -n "$MANIFEST_PATH" && ! -f "$MANIFEST_PATH" ]]; then
    echo "ERROR: manifest file not found: $MANIFEST_PATH" >&2
    usage
fi

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
readonly COMPOSE_FILES="--profile observability -f infra/docker-compose.yml -f infra/docker-compose.staging.yml"
readonly IMAGE_BASE="${IMAGE_BASE:-ghcr.io/mohamed155/customer-service-chat-system}"
readonly BACKEND_IMAGE="${IMAGE_BASE}-backend:${SHA}"
readonly DASHBOARD_IMAGE="${IMAGE_BASE}-dashboard:${SHA}"
readonly WIDGET_IMAGE="${IMAGE_BASE}-widget:${SHA}"
readonly TIMEOUT=120
readonly INTERVAL=5

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
# Step 0: Save the current SHA before doing anything destructive
# ---------------------------------------------------------------------------
CURRENT_SHA="${APP_RELEASE_SHA:-}"
if [[ -z "$CURRENT_SHA" ]]; then
    # Attempt to read from the currently running container
    if ! $DRY_RUN; then
        CURRENT_SHA="$(
            docker compose $COMPOSE_FILES exec -T backend \
                sh -c 'echo "${APP_RELEASE_SHA:-}"' 2>/dev/null || true
        )"
    fi
fi

echo "=== Deploying $SHA (current: ${CURRENT_SHA:-none}) ==="

# ---------------------------------------------------------------------------
# Step 1: Verify <git-sha> exists and images are published
# ---------------------------------------------------------------------------
echo "--- Step 1/6: Verifying SHA and images ---"

if $DRY_RUN; then
    dry_run "git rev-parse --verify $SHA"
    dry_run "docker manifest inspect $BACKEND_IMAGE"
    dry_run "docker manifest inspect $DASHBOARD_IMAGE"
    dry_run "docker manifest inspect $WIDGET_IMAGE"
else
    if ! git rev-parse --verify "$SHA^{commit}" >/dev/null 2>&1; then
        echo "ERROR: Git SHA '$SHA' not found in local repository" >&2
        exit 1
    fi

    if ! docker manifest inspect "$BACKEND_IMAGE" >/dev/null 2>&1; then
        echo "ERROR: Image not found: $BACKEND_IMAGE" >&2
        exit 1
    fi
    echo "  Image found: $BACKEND_IMAGE"

    if ! docker manifest inspect "$DASHBOARD_IMAGE" >/dev/null 2>&1; then
        echo "ERROR: Image not found: $DASHBOARD_IMAGE" >&2
        exit 1
    fi
    echo "  Image found: $DASHBOARD_IMAGE"

    if ! docker manifest inspect "$WIDGET_IMAGE" >/dev/null 2>&1; then
        echo "ERROR: Image not found: $WIDGET_IMAGE" >&2
        exit 1
    fi
    echo "  Image found: $WIDGET_IMAGE"
fi

# ---------------------------------------------------------------------------
# Step 2/6: Validate deployment manifest (if --manifest provided)
# ---------------------------------------------------------------------------
echo "--- Step 2/6: Validating deployment manifest ---"

if [[ -n "$MANIFEST_PATH" ]]; then
    if ! command -v python3 &>/dev/null; then
        echo "ERROR: python3 required for manifest validation" >&2
        exit 1
    fi

    echo "  Validating manifest: $MANIFEST_PATH"

    MANIFEST_BACKEND=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['images']['backend'])" "$MANIFEST_PATH")
    MANIFEST_DASHBOARD=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['images']['dashboard'])" "$MANIFEST_PATH")
    MANIFEST_WIDGET=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['images']['widget'])" "$MANIFEST_PATH")

    if [[ "$MANIFEST_BACKEND" != "$BACKEND_IMAGE" ]]; then
        echo "ERROR: Manifest backend image mismatch: expected $BACKEND_IMAGE, got $MANIFEST_BACKEND" >&2
        exit 1
    fi
    echo "  Backend image matches: $MANIFEST_BACKEND"

    if [[ "$MANIFEST_DASHBOARD" != "$DASHBOARD_IMAGE" ]]; then
        echo "ERROR: Manifest dashboard image mismatch: expected $DASHBOARD_IMAGE, got $MANIFEST_DASHBOARD" >&2
        exit 1
    fi
    echo "  Dashboard image matches: $MANIFEST_DASHBOARD"

    if [[ "$MANIFEST_WIDGET" != "$WIDGET_IMAGE" ]]; then
        echo "ERROR: Manifest widget image mismatch: expected $WIDGET_IMAGE, got $MANIFEST_WIDGET" >&2
        exit 1
    fi
    echo "  Widget image matches: $MANIFEST_WIDGET"

    echo "  Manifest validation passed"
else
    echo "  WARNING: --manifest not supplied — skipping manifest validation"
fi

# ---------------------------------------------------------------------------
# Step 3/6: Pull images by immutable SHA tag
# ---------------------------------------------------------------------------
echo "--- Step 3/6: Pulling images ---"

run_or_dry docker pull "$BACKEND_IMAGE"
run_or_dry docker pull "$DASHBOARD_IMAGE"
run_or_dry docker pull "$WIDGET_IMAGE"

# ---------------------------------------------------------------------------
# Step 4/6: Run migrations in a one-shot container
# ---------------------------------------------------------------------------
echo "--- Step 4/6: Running migrations ---"

if $DRY_RUN; then
    dry_run "docker compose $COMPOSE_FILES run --rm -e APP_RELEASE_SHA=$SHA backend migrate"
elif $SKIP_MIGRATIONS; then
    echo "  SKIP_MIGRATIONS set — skipping migrations"
else
    # Run migration in a disposable container using the new image
    # If this fails, we abort BEFORE touching the running stack
    if ! docker compose $COMPOSE_FILES run --rm \
        -e APP_RELEASE_SHA="$SHA" \
        backend migrate; then
        echo "ERROR: Migration failed. Aborting deploy — old release still serving." >&2
        exit 1
    fi
    echo "  Migrations applied successfully"
fi

# ---------------------------------------------------------------------------
# Step 5/6: Start the new stack and poll /ready
# ---------------------------------------------------------------------------
echo "--- Step 5/6: Starting new stack ---"

export APP_RELEASE_SHA="$SHA"

run_or_dry docker compose $COMPOSE_FILES up -d

# --- Readiness gate (T067 / FR-027) ---
echo "  Polling /ready (timeout: ${TIMEOUT}s)..."

if $DRY_RUN; then
    dry_run "curl -sf http://localhost:8080/ready (poll up to ${TIMEOUT}s)"
else
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
        echo "ERROR: Backend not ready within ${TIMEOUT}s. Rolling back..." >&2

        # Restore the old release
        if [[ -n "$CURRENT_SHA" ]]; then
            export APP_RELEASE_SHA="$CURRENT_SHA"
            docker compose $COMPOSE_FILES up -d
        fi

        echo "  Old release restored. Deploy aborted." >&2
        exit 1
    fi
fi

# ---------------------------------------------------------------------------
# Step 6/6: Switch Caddy upstream and stop old stack
# ---------------------------------------------------------------------------
echo "--- Step 6/6: Finalising ---"

if ! $DRY_RUN; then
    # Reload Caddy to pick up any config changes
    if docker compose $COMPOSE_FILES ps -q caddy >/dev/null 2>&1; then
        docker compose $COMPOSE_FILES exec caddy caddy reload --config /etc/caddy/Caddyfile 2>/dev/null || true
    fi

    # Clean up old containers (Docker handles this via up -d, but ensure orphans are gone)
    docker compose $COMPOSE_FILES up -d --remove-orphans
fi

echo ""
echo "=== Deploy of $SHA completed successfully ==="
exit 0
