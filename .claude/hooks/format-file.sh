#!/usr/bin/env bash
# PostToolUse hook (Edit|Write): format the file Claude just changed, the way CI checks it.
#   *.rs under backend/   -> rustfmt (pinned toolchain from backend/rust-toolchain.toml)
#   files under frontend/ -> prettier from frontend/node_modules (respects .prettierignore)
# Never blocks: a formatter failure is reported on stderr and the hook exits 0.
set -u

root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)}"
file="$(jq -r '.tool_input.file_path // .tool_response.filePath // empty')"
[ -n "$file" ] && [ -f "$file" ] || exit 0

case "$file" in
  "$root"/backend/*.rs)
    (cd "$root/backend" && rustfmt --edition 2021 "$file") || echo "format-file: rustfmt failed on $file" >&2
    ;;
  "$root"/frontend/*)
    case "$file" in */node_modules/*|*/dist/*|*/.angular/*) exit 0 ;; esac
    (cd "$root/frontend" && ./node_modules/.bin/prettier --ignore-unknown --log-level warn --write "$file") \
      || echo "format-file: prettier failed on $file" >&2
    ;;
esac
exit 0
