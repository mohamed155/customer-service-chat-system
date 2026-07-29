#!/bin/sh
# Expand env vars in alertmanager config template, then start alertmanager.
# Alertmanager does not expand ${VAR} natively (no url_file support in v0.28.0).
# busybox (the base image) has no env-substitution tool, so use sed.
set -e
if [ -z "${APP_ALERT_WEBHOOK_URL:-}" ]; then
  echo "APP_ALERT_WEBHOOK_URL is not set - alerts would go nowhere" >&2
  exit 1
fi
sed "s|\${APP_ALERT_WEBHOOK_URL}|${APP_ALERT_WEBHOOK_URL}|g" \
  /etc/alertmanager/alertmanager.yml.tmpl > /tmp/alertmanager.yml
exec /bin/alertmanager "$@"
