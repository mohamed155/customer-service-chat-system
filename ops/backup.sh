#!/usr/bin/env bash
set -euo pipefail

# ops/backup.sh — Nightly backup of database and object storage to off-host destination.
#
# Usage: $0 [--target db|objects|all] [--dry-run]
#
# Exit codes: 0 success, 1 failure, 2 misuse
#
# Environment:
#   BACKUP_S3_ENDPOINT       (required) Off-host S3 endpoint
#   BACKUP_S3_BUCKET          (required) Off-host S3 bucket
#   BACKUP_S3_ACCESS_KEY      (required) Access key for off-host S3
#   BACKUP_S3_SECRET_KEY      (required) Secret key for off-host S3
#   BACKUP_AGE_PUBLIC_KEY     (required) age public key for encryption
#   BACKUP_RETENTION_DAYS     (optional, default 30)
#   APP_S3_ENDPOINT           (required) Local MinIO endpoint
#   APP_S3_BUCKET             (required) Local MinIO bucket
#   APP_S3_ACCESS_KEY         (required) Local MinIO access key
#   APP_S3_SECRET_KEY         (required) Local MinIO secret key
#   PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSWORD  (standard PG env vars). PGUSER must be the
#                            bootstrap superuser: pg_dump cannot dump tables with forced
#                            Row-Level Security as app_owner or app_runtime.
#   NODE_EXPORTER_TEXTFILE_DIR  (optional, default /var/lib/node_exporter/textfile_collector)

readonly SCRIPT_NAME=$(basename "$0")

log()  { echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*"; }
error(){ log "ERROR: $*" >&2; }

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
TARGET="all"
DRY_RUN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --dry-run) DRY_RUN=true; shift ;;
    --help|-h)
      echo "Usage: $SCRIPT_NAME [--target db|objects|all] [--dry-run]"
      echo ""
      echo "Backup database and/or objects to off-host storage."
      echo ""
      echo "  --target    What to back up: db, objects, or all (default: all)"
      echo "  --dry-run   Print what would be done without doing it"
      exit 0 ;;
    *) error "Unknown argument: $1"; exit 2 ;;
  esac
done

case "$TARGET" in
  db|objects|all) ;;
  *) error "Invalid --target: must be db, objects, or all (got: $TARGET)"; exit 2 ;;
esac

# ---------------------------------------------------------------------------
# Required environment validation
# ---------------------------------------------------------------------------
: "${BACKUP_S3_ENDPOINT:?Must be set to the off-host S3 endpoint}"
: "${BACKUP_S3_BUCKET:?Must be set}"
: "${BACKUP_S3_ACCESS_KEY:?Must be set}"
: "${BACKUP_S3_SECRET_KEY:?Must be set}"
: "${BACKUP_AGE_PUBLIC_KEY:?Must be set to the age public key}"
: "${APP_S3_ENDPOINT:?Must be set to the local MinIO endpoint}"
: "${APP_S3_BUCKET:?Must be set}"
: "${APP_S3_ACCESS_KEY:?Must be set}"
: "${APP_S3_SECRET_KEY:?Must be set}"

RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-30}"
TIMESTAMP=$(date -u +%Y%m%dT%H%M%SZ)
BACKUP_FILENAME="backup-${TIMESTAMP}.dump.age"
METRIC_DIR="${NODE_EXPORTER_TEXTFILE_DIR:-/var/lib/node_exporter/textfile_collector}"
METRIC_FILE="${METRIC_DIR}/backup.prom"
WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

# ---------------------------------------------------------------------------
# FR-028: Reject destination that resolves to the local MinIO endpoint
# ---------------------------------------------------------------------------
resolve_ip() {
  getent hosts "$1" 2>/dev/null | awk '{print $1; exit}'
}

local_host="${APP_S3_ENDPOINT#*://}"; local_host="${local_host%%:*}"
backup_host="${BACKUP_S3_ENDPOINT#*://}"; backup_host="${backup_host%%:*}"

# Direct localhost/loopback check (catches plain hostname patterns)
if echo "$backup_host" | grep -qiE '^(localhost|127\.0\.0\.1|::1)$'; then
  error "Backup destination ($BACKUP_S3_ENDPOINT) is localhost. Off-host storage required (FR-028)."
  exit 1
fi

# DNS resolution comparison
local_ip=$(resolve_ip "$local_host" || true)
backup_ip=$(resolve_ip "$backup_host" || true)

if [[ -n "$local_ip" && -n "$backup_ip" && "$local_ip" == "$backup_ip" ]]; then
  error "Backup destination ($BACKUP_S3_ENDPOINT) resolves to same IP ($backup_ip) as local MinIO ($APP_S3_ENDPOINT). Off-host storage required (FR-028)."
  exit 1
fi

# ---------------------------------------------------------------------------
# Configure mc aliases
# ---------------------------------------------------------------------------
if ! $DRY_RUN; then
  mc alias set local  "$APP_S3_ENDPOINT"  "$APP_S3_ACCESS_KEY"  "$APP_S3_SECRET_KEY"  >/dev/null
  mc alias set remote "$BACKUP_S3_ENDPOINT" "$BACKUP_S3_ACCESS_KEY" "$BACKUP_S3_SECRET_KEY" >/dev/null
fi

log "Starting backup [target=$TARGET, timestamp=$TIMESTAMP]"

# ===========================================================================
# 1. Mirror objects FIRST (FR-029 consistency: objects before DB)
# ===========================================================================
if [[ "$TARGET" =~ ^(objects|all)$ ]]; then
  object_path="objects/${TIMESTAMP}/"
  log "Mirroring objects to remote: ${BACKUP_S3_BUCKET}/${object_path}"

  if $DRY_RUN; then
    echo "[DRY-RUN] nice -n 19 ionice -c 2 -n 7 mc mirror --overwrite local/${APP_S3_BUCKET} remote/${BACKUP_S3_BUCKET}/${object_path}"
  else
    nice -n 19 ionice -c 2 -n 7 \
      mc mirror --overwrite "local/${APP_S3_BUCKET}" "remote/${BACKUP_S3_BUCKET}/${object_path}"
    log "Objects mirror complete"
  fi
fi

# ===========================================================================
# 2. Dump and encrypt database SECOND
# ===========================================================================
if [[ "$TARGET" =~ ^(db|all)$ ]]; then
  log "Dumping database to encrypted backup: ${BACKUP_FILENAME}"

  if $DRY_RUN; then
    echo "[DRY-RUN] nice -n 19 ionice -c 2 -n 7 pg_dump --format=custom --no-owner --compress=6 | age --encrypt --recipient ... > ${BACKUP_FILENAME}"
    echo "[DRY-RUN] mc cp ... remote/${BACKUP_S3_BUCKET}/db/${BACKUP_FILENAME}"
  else
    # Tables under forced Row-Level Security cannot be dumped by a role the policies apply to:
    # pg_dump stops with an error. app_owner and app_runtime are such roles; the bootstrap
    # superuser (POSTGRES_USER) is not.
    if [[ "$(psql -AtX -c "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = current_user")" != "t" ]]; then
      error "PGUSER must be a role that bypasses Row-Level Security (the bootstrap superuser): pg_dump cannot dump tables with forced policies as app_owner or app_runtime"
      exit 1
    fi
    nice -n 19 ionice -c 2 -n 7 \
      pg_dump --format=custom --no-owner --compress=6 \
      | age --encrypt --recipient "$BACKUP_AGE_PUBLIC_KEY" \
      > "$WORK_DIR/$BACKUP_FILENAME"

    mc cp "$WORK_DIR/$BACKUP_FILENAME" "remote/${BACKUP_S3_BUCKET}/db/${BACKUP_FILENAME}"
    log "Database backup uploaded"
  fi
fi

# ===========================================================================
# 3. Write Prometheus metric (textfile collector)
# ===========================================================================
if ! $DRY_RUN; then
  mkdir -p "$(dirname "$METRIC_FILE")"
  cat > "$METRIC_FILE" <<-PROM
# HELP backup_last_success_timestamp_seconds Unix timestamp of last successful backup
# TYPE backup_last_success_timestamp_seconds gauge
backup_last_success_timestamp_seconds $(date +%s)
PROM
  log "Metric written to ${METRIC_FILE}"
fi

# ===========================================================================
# 4. Retention pruning (remove backups older than RETENTION_DAYS)
# ===========================================================================
if ! $DRY_RUN; then
  log "Pruning backups older than ${RETENTION_DAYS} days"

  cutoff_epoch=$(date -u -d "${RETENTION_DAYS} days ago" +%s)

  # List remote backup files and remove old ones
  mc ls "remote/${BACKUP_S3_BUCKET}/db/" 2>/dev/null | while read -r line; do
    file_date=$(echo "$line" | grep -oE '[0-9]{8}T[0-9]{6}Z' || true)
    if [[ -n "$file_date" ]]; then
      file_epoch=$(date -u -d "$(echo "$file_date" | sed 's/T/ /; s/Z//')" +%s 2>/dev/null || true)
      if [[ -n "$file_epoch" && "$file_epoch" -lt "$cutoff_epoch" ]]; then
        file_name=$(echo "$line" | awk '{print $NF}')
        mc rm "remote/${BACKUP_S3_BUCKET}/db/${file_name}"
        log "Pruned expired backup: ${file_name}"
      fi
    fi
  done

  # Also prune old object mirrors
  mc ls "remote/${BACKUP_S3_BUCKET}/objects/" 2>/dev/null | while read -r line; do
    dir_date=$(echo "$line" | grep -oE '[0-9]{8}T[0-9]{6}Z' || true)
    if [[ -n "$dir_date" ]]; then
      dir_epoch=$(date -u -d "$(echo "$dir_date" | sed 's/T/ /; s/Z//')" +%s 2>/dev/null || true)
      if [[ -n "$dir_epoch" && "$dir_epoch" -lt "$cutoff_epoch" ]]; then
        dir_name=$(echo "$line" | awk '{print $NF}' | tr -d '/')
        mc rm --recursive "remote/${BACKUP_S3_BUCKET}/objects/${dir_name}/"
        log "Pruned expired object mirror: ${dir_name}"
      fi
    fi
  done
fi

log "Backup completed successfully"
exit 0
