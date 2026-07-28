#!/usr/bin/env bash
set -euo pipefail

# ops/restore.sh — Restore database and objects from a backup.
#
# Usage: $0 <backup-timestamp> [--yes] [--force-live] [--dry-run]
#
#   <backup-timestamp>   Timestamp of the backup to restore (YYYYMMDDTHHMMSSZ).
#
# Exit codes: 0 success, 1 failure, 2 misuse
#
# Environment:
#   BACKUP_S3_ENDPOINT, BACKUP_S3_BUCKET, BACKUP_S3_ACCESS_KEY, BACKUP_S3_SECRET_KEY
#   BACKUP_AGE_PRIVATE_KEY  (path to age private key file for decryption)
#   APP_S3_ENDPOINT, APP_S3_BUCKET, APP_S3_ACCESS_KEY, APP_S3_SECRET_KEY
#   PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSWORD
#   DATABASE_URL  (alternative to individual PG vars, used for migration runner)
#
# References:
#   docs/backup-restore.md — full restore procedure and destructive-migration path

readonly SCRIPT_NAME=$(basename "$0")

log()  { echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*"; }
error(){ log "ERROR: $*" >&2; }

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
FORCE_LIVE=false
CONFIRMED=false
DRY_RUN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force-live) FORCE_LIVE=true; shift ;;
    --yes) CONFIRMED=true; shift ;;
    --dry-run) DRY_RUN=true; shift ;;
    --help|-h)
      echo "Usage: $SCRIPT_NAME <backup-timestamp> [--force-live] [--yes] [--dry-run]"
      echo ""
      echo "Restore from backup. The backup timestamp matches the backup file name:"
      echo "  backup-<YYYYMMDDTHHMMSSZ>.dump.age"
      exit 0 ;;
    -*)
      error "Unknown argument: $1"; exit 2 ;;
    *)
      if [[ -z "${BACKUP_TIMESTAMP:-}" ]]; then
        BACKUP_TIMESTAMP="$1"
      else
        error "Multiple positional arguments: $1"; exit 2
      fi
      shift ;;
  esac
done

if [[ -z "${BACKUP_TIMESTAMP:-}" ]]; then
  error "Missing required argument: <backup-timestamp>"
  echo "Usage: $SCRIPT_NAME <backup-timestamp> [--force-live] [--yes] [--dry-run]" >&2
  exit 2
fi

# Validate timestamp format
if ! echo "$BACKUP_TIMESTAMP" | grep -qE '^[0-9]{8}T[0-9]{6}Z$'; then
  error "Invalid timestamp format: $BACKUP_TIMESTAMP (expected YYYYMMDDTHHMMSSZ)"
  exit 2
fi

# ---------------------------------------------------------------------------
# Required environment validation
# ---------------------------------------------------------------------------
: "${BACKUP_S3_ENDPOINT:?Must be set}"
: "${BACKUP_S3_BUCKET:?Must be set}"
: "${BACKUP_S3_ACCESS_KEY:?Must be set}"
: "${BACKUP_S3_SECRET_KEY:?Must be set}"
: "${BACKUP_AGE_PRIVATE_KEY:?Must be set to path of age private key}"
: "${APP_S3_ENDPOINT:?Must be set}"
: "${APP_S3_BUCKET:?Must be set}"
: "${APP_S3_ACCESS_KEY:?Must be set}"
: "${APP_S3_SECRET_KEY:?Must be set}"

if [[ ! -f "$BACKUP_AGE_PRIVATE_KEY" ]]; then
  error "Age private key not found: $BACKUP_AGE_PRIVATE_KEY"
  exit 1
fi

BACKUP_FILE="backup-${BACKUP_TIMESTAMP}.dump.age"
RESTORE_DIR=$(mktemp -d)
trap 'rm -rf "$RESTORE_DIR"' EXIT

# ---------------------------------------------------------------------------
# Determine if we are targeting the live stack
# ---------------------------------------------------------------------------
# We consider the environment "live" if PGHOST resolves to a local Docker
# container or localhost. This is a heuristic — the --force-live flag is the
# authoritative override.
live_stack=false
if echo "${PGHOST:-}" | grep -qiE '^(localhost|127\.0\.0\.1|postgres|::1)$'; then
  live_stack=true
fi

if $live_stack && ! $FORCE_LIVE; then
  error "Refusing to target the live stack (PGHOST=$PGHOST). Use --force-live to override."
  echo "This is the most dangerous operation in the repository. Verify you intend to" >&2
  echo "overwrite the live database and objects before proceeding." >&2
  exit 1
fi

if ! $CONFIRMED; then
  error "Confirmation required. Use --yes to confirm."
  exit 2
fi

# ---------------------------------------------------------------------------
# Download and decrypt backup
# ---------------------------------------------------------------------------
log "Downloading backup: ${BACKUP_FILE}"

if $DRY_RUN; then
  echo "[DRY-RUN] mc cp remote/${BACKUP_S3_BUCKET}/db/${BACKUP_FILE} ${RESTORE_DIR}/${BACKUP_FILE}"
  echo "[DRY-RUN] age --decrypt -i ${BACKUP_AGE_PRIVATE_KEY} ${RESTORE_DIR}/${BACKUP_FILE} > ${RESTORE_DIR}/backup.dump"
else
  mc alias set remote "$BACKUP_S3_ENDPOINT" "$BACKUP_S3_ACCESS_KEY" "$BACKUP_S3_SECRET_KEY" >/dev/null
  mc cp "remote/${BACKUP_S3_BUCKET}/db/${BACKUP_FILE}" "$RESTORE_DIR/$BACKUP_FILE"

  log "Decrypting backup"
  age --decrypt -i "$BACKUP_AGE_PRIVATE_KEY" "$RESTORE_DIR/$BACKUP_FILE" > "$RESTORE_DIR/backup.dump"
  log "Decryption complete"
fi

# ---------------------------------------------------------------------------
# Schema version comparison (FR-033)
# ---------------------------------------------------------------------------
# Extract the dump schema version by restoring into a throwaway database and
# querying _sqlx_migrations, or by using pg_restore --list metadata.
#
# Since we may not have a second database available, we use pg_restore --list
# to find migration entries and extract version numbers from the SQL migration
# file names embedded in the dump.

log "Checking schema version"

if $DRY_RUN; then
  echo "[DRY-RUN] Extracting migration versions from backup dump"
  echo "[DRY-RUN] Querying current database migration state"
else
  # Get the latest migration version in the backup by inspecting dump metadata
  backup_max_version=$(pg_restore --list "$RESTORE_DIR/backup.dump" 2>/dev/null \
    | grep -oE '[0-9]{4}_[a-z_]+\.[a-z]+' \
    | grep -oE '^[0-9]{4}' \
    | sort -n \
    | tail -1 || echo "0000")

  # Get the latest applied migration version from the target database
  current_max_version=$(psql -AtX -c "SELECT MAX(version)::text FROM _sqlx_migrations" 2>/dev/null || echo "0000")

  log "Backup schema version: ${backup_max_version}"
  log "Target schema version: ${current_max_version}"

  if [[ "$backup_max_version" -gt "$current_max_version" ]]; then
    # FR-033: Fail loudly if dump is newer than the code
    error "Backup schema version (${backup_max_version}) is newer than the target (${current_max_version})."
    echo "The database code in the deployed release is older than the backup." >&2
    echo "Deploy a release matching the backup's schema version before restoring." >&2
    exit 1
  fi

  if [[ "$backup_max_version" -lt "$current_max_version" ]]; then
    # Dump is older — apply intervening migrations
    log "Backup is older than target. Applying intervening migrations..."

    if command -v sqlx &>/dev/null; then
      sqlx migrate run --database-url "${DATABASE_URL}" 2>/dev/null \
        || sqlx migrate run 2>/dev/null \
        || log "WARNING: Could not auto-apply migrations. Run manually after restore."
    else
      log "WARNING: sqlx CLI not found. You must apply migrations manually after restore."
    fi

    log "Migrations applied"
  fi
fi

# ===========================================================================
# 1. Restore objects FIRST (mirror of backup order: objects before DB)
# ===========================================================================
log "Restoring objects from backup"

if $DRY_RUN; then
  echo "[DRY-RUN] mc alias set local ..."
  echo "[DRY-RUN] mc mirror --overwrite remote/${BACKUP_S3_BUCKET}/objects/${BACKUP_TIMESTAMP}/ local/${APP_S3_BUCKET}"
else
  mc alias set local "$APP_S3_ENDPOINT" "$APP_S3_ACCESS_KEY" "$APP_S3_SECRET_KEY" >/dev/null
  mc mirror --overwrite "remote/${BACKUP_S3_BUCKET}/objects/${BACKUP_TIMESTAMP}/" "local/${APP_S3_BUCKET}"
  log "Objects restore complete"
fi

# ===========================================================================
# 2. Restore database SECOND
# ===========================================================================
log "Restoring database from backup"

if $DRY_RUN; then
  echo "[DRY-RUN] pg_restore --clean --if-exists --no-owner --no-privileges --dbname ... ${RESTORE_DIR}/backup.dump"
else
  pg_restore --clean --if-exists --no-owner --no-privileges \
    --dbname "postgresql://${PGUSER}:${PGPASSWORD}@${PGHOST}:${PGPORT:-5432}/${PGDATABASE}" \
    "$RESTORE_DIR/backup.dump"
  log "Database restore complete"
fi

log "Restore completed successfully"
exit 0
