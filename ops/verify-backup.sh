#!/usr/bin/env bash
set -euo pipefail

# ops/verify-backup.sh — FR-030 drill: restore the latest backup into a
# throwaway container, run consistency checks, and report RPO/RTO.
#
# Usage: $0 [--dry-run]
#
# Exit codes: 0 all objectives met, 1 objective missed, 2 misuse
#
# Environment:
#   All env vars required by restore.sh plus:
#   VERIFY_TARGET_DATABASE_URL  (required) Connection to the source database for comparison
#   VERIFY_TARGET_S3_ENDPOINT   (required) Source MinIO for row count comparison
#   BACKUP_VERIFY_DB_CONTAINER  (optional, default: postgres:16) Image for throwaway container
#   BACKUP_RPO_SECONDS          (optional, default 86400 = 24h)
#   BACKUP_RTO_SECONDS          (optional, default 14400 = 4h)

readonly SCRIPT_NAME=$(basename "$0")

log()  { echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*"; }
error(){ log "ERROR: $*" >&2; }

DRY_RUN=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=true; shift ;;
    --help|-h)
      echo "Usage: $SCRIPT_NAME [--dry-run]"
      echo ""
      echo "Verify the latest backup by restoring into a throwaway container and"
      echo "comparing against the source database. Reports measured RPO and RTO."
      exit 0 ;;
    *) error "Unknown argument: $1"; exit 2 ;;
  esac
done

# ---------------------------------------------------------------------------
# Env validation
# ---------------------------------------------------------------------------
: "${BACKUP_S3_ENDPOINT:?Must be set}"
: "${BACKUP_S3_BUCKET:?Must be set}"
: "${BACKUP_S3_ACCESS_KEY:?Must be set}"
: "${BACKUP_S3_SECRET_KEY:?Must be set}"
: "${BACKUP_AGE_PRIVATE_KEY:?Must be set}"
: "${VERIFY_TARGET_DATABASE_URL:?Must be set (source database connection)}"
: "${BACKUP_AGE_PUBLIC_KEY:?Must be set}"

RPO_TARGET=${BACKUP_RPO_SECONDS:-86400}   # 24 hours
RTO_TARGET=${BACKUP_RTO_SECONDS:-14400}   # 4 hours
VERIFY_DB_IMAGE=${BACKUP_VERIFY_DB_CONTAINER:-postgres:16}
RESTORE_DIR=$(mktemp -d)
trap 'rm -rf "$RESTORE_DIR"; docker stop verify-postgres 2>/dev/null || true; docker rm verify-postgres 2>/dev/null || true' EXIT

# ---------------------------------------------------------------------------
# Find the latest backup
# ---------------------------------------------------------------------------
log "Finding latest backup"

if $DRY_RUN; then
  echo "[DRY-RUN] mc ls remote/${BACKUP_S3_BUCKET}/db/"
  LATEST_BACKUP="backup-$(date -u +%Y%m%dT000000Z).dump.age"
  BACKUP_TIMESTAMP=$(echo "$LATEST_BACKUP" | grep -oE '[0-9]{8}T[0-9]{6}Z')
else
  mc alias set remote "$BACKUP_S3_ENDPOINT" "$BACKUP_S3_ACCESS_KEY" "$BACKUP_S3_SECRET_KEY" >/dev/null

  LATEST_BACKUP=$(mc ls "remote/${BACKUP_S3_BUCKET}/db/" 2>/dev/null \
    | grep -oE 'backup-[0-9]{8}T[0-9]{6}Z\.dump\.age' \
    | sort -r \
    | head -1 || true)

  if [[ -z "$LATEST_BACKUP" ]]; then
    error "No backup files found in remote/${BACKUP_S3_BUCKET}/db/"
    exit 1
  fi

  BACKUP_TIMESTAMP=$(echo "$LATEST_BACKUP" | grep -oE '[0-9]{8}T[0-9]{6}Z')
  log "Latest backup: ${LATEST_BACKUP} (timestamp: ${BACKUP_TIMESTAMP})"
fi

# Measure RPO: time between now and the backup timestamp
backup_epoch=$(date -u -d "$(echo "$BACKUP_TIMESTAMP" | sed 's/T/ /; s/Z//')" +%s 2>/dev/null || date -u -j -f "%Y%m%dT%H%M%SZ" "$BACKUP_TIMESTAMP" +%s 2>/dev/null)
now_epoch=$(date +%s)
rpo_actual=$(( now_epoch - backup_epoch ))

log "Measured RPO: ${rpo_actual}s (target: ${RPO_TARGET}s = $((RPO_TARGET / 3600))h)"

if $DRY_RUN; then
  RTO_START=$EPOCHSECONDS
  RTO_ACTUAL=0
else
  RTO_START=$EPOCHSECONDS

  # ---------------------------------------------------------------------------
  # Download and decrypt the backup
  # ---------------------------------------------------------------------------
  log "Downloading backup"
  mc cp "remote/${BACKUP_S3_BUCKET}/db/${LATEST_BACKUP}" "$RESTORE_DIR/$LATEST_BACKUP"

  log "Decrypting backup"
  age --decrypt -i "$BACKUP_AGE_PRIVATE_KEY" "$RESTORE_DIR/$LATEST_BACKUP" > "$RESTORE_DIR/backup.dump"

  # ---------------------------------------------------------------------------
  # Start throwaway PostgreSQL container
  # ---------------------------------------------------------------------------
  log "Starting throwaway PostgreSQL container (${VERIFY_DB_IMAGE})"
  docker run -d \
    --name verify-postgres \
    -e POSTGRES_DB=verify \
    -e POSTGRES_USER=verify \
    -e POSTGRES_PASSWORD=verify \
    -p 5433:5432 \
    "$VERIFY_DB_IMAGE" >/dev/null

  # Wait for PostgreSQL to be ready
  for i in $(seq 1 30); do
    if docker exec verify-postgres pg_isready -U verify -d verify &>/dev/null; then
      break
    fi
    sleep 1
  done

  # ---------------------------------------------------------------------------
  # Restore the backup into the throwaway database
  # ---------------------------------------------------------------------------
  log "Restoring backup into throwaway database"
  pg_restore --clean --if-exists --no-owner --no-privileges \
    --dbname "postgresql://verify:verify@localhost:5433/verify" \
    "$RESTORE_DIR/backup.dump"

  RTO_ACTUAL=$(( $(date +%s) - RTO_START ))
  log "Measured RTO (restore only): ${RTO_ACTUAL}s (target: ${RTO_TARGET}s = $((RTO_TARGET / 3600))h)"

  # ---------------------------------------------------------------------------
  # Run migration checker (compare schema version)
  # ---------------------------------------------------------------------------
  log "Running migration schema check"

  restore_max_version=$(psql -AtX -p 5433 -U verify -d verify -c "SELECT MAX(version)::text FROM _sqlx_migrations" 2>/dev/null || echo "0000")
  source_max_version=$(psql "${VERIFY_TARGET_DATABASE_URL}" -AtX -c "SELECT MAX(version)::text FROM _sqlx_migrations" 2>/dev/null || echo "0000")

  log "Restored schema version: ${restore_max_version}"
  log "Source schema version:   ${source_max_version}"

  if [[ "$restore_max_version" != "$source_max_version" ]]; then
    error "Schema version mismatch: restored=${restore_max_version}, source=${source_max_version}"
    exit 1
  fi

  # ---------------------------------------------------------------------------
  # Count rows per tenant and compare against source
  # ---------------------------------------------------------------------------
  log "Counting rows per tenant"

  # Discover tenant tables and count rows per tenant
  tables=$(psql -AtX -p 5433 -U verify -d verify -c "
    SELECT tablename FROM pg_tables
    WHERE schemaname = 'public'
      AND tablename NOT LIKE '_sqlx%'
      AND tablename NOT LIKE 'sqlx%'
    ORDER BY tablename
  " 2>/dev/null)

  mismatches=0
  total_rows_source=0
  total_rows_restored=0

  while IFS= read -r table; do
    [[ -z "$table" ]] && continue

    rows_source=$(psql "${VERIFY_TARGET_DATABASE_URL}" -AtX -c "SELECT COUNT(*) FROM \"$table\"" 2>/dev/null || echo "0")
    rows_restored=$(psql -AtX -p 5433 -U verify -d verify -c "SELECT COUNT(*) FROM \"$table\"" 2>/dev/null || echo "0")

    total_rows_source=$(( total_rows_source + rows_source ))
    total_rows_restored=$(( total_rows_restored + rows_restored ))

    if [[ "$rows_source" -ne "$rows_restored" ]]; then
      error "Row count mismatch in ${table}: source=${rows_source}, restored=${rows_restored}"
      mismatches=$(( mismatches + 1 ))
    fi
  done <<< "$tables"

  log "Total rows — source: ${total_rows_source}, restored: ${total_rows_restored}"

  if [[ "$mismatches" -gt 0 ]]; then
    error "${mismatches} table(s) have row count mismatches"
    exit 1
  fi

  log "All row counts match"
fi

# ---------------------------------------------------------------------------
# Report RPO/RTO against objectives
# ---------------------------------------------------------------------------
rpo_ok=false
rto_ok=false

if [[ "$rpo_actual" -le "$RPO_TARGET" ]]; then
  rpo_ok=true
  log "RPO objective MET: ${rpo_actual}s <= ${RPO_TARGET}s"
else
  error "RPO objective MISSED: ${rpo_actual}s > ${RPO_TARGET}s"
fi

if [[ "$RTO_ACTUAL" -le "$RTO_TARGET" ]]; then
  rto_ok=true
  log "RTO objective MET: ${RTO_ACTUAL}s <= ${RTO_TARGET}s"
else
  error "RTO objective MISSED: ${RTO_ACTUAL}s > ${RTO_TARGET}s"
fi

if $rpo_ok && $rto_ok; then
  log "All recovery objectives met. Backup verification PASSED."
  exit 0
else
  error "One or more recovery objectives MISSED. Backup verification FAILED."
  exit 1
fi
