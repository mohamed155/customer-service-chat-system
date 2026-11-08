#!/bin/sh
# MinIO bootstrap: creates the buckets the platform expects and keeps them private.
#
# Idempotent. After the buckets exist it writes /tmp/buckets-ready (the compose healthcheck) and
# stays alive: `docker compose up --wait` treats a container that has exited as a failure, even
# with exit code 0, so a one-shot container would make `just up` report an error.
set -eu

mc alias set local "http://minio:9000" "${MINIO_ROOT_USER}" "${MINIO_ROOT_PASSWORD}" >/dev/null

for bucket in tenants quarantine; do
  mc mb --ignore-existing "local/${bucket}"
  mc anonymous set none "local/${bucket}" >/dev/null
  echo "bucket ready: ${bucket}"
done

touch /tmp/buckets-ready

# Exit promptly on `docker compose stop` (sleep as PID 1 would ignore SIGTERM for 10 s).
trap 'exit 0' TERM INT
while :; do
  sleep 3600 &
  wait $!
done
