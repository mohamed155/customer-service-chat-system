#!/usr/bin/env bash
# PreToolUse hook (Edit|Write): refuse edits to files Claude must not change by hand.
#   .env, .env.local, ...      real local secrets (.env.example stays editable)
#   Cargo.lock, pnpm-lock.yaml change only through cargo / pnpm
#   backend/migrations/*.sql   already on master: editing an applied migration breaks the
#                              SQLx checksum on every database that ran it. Add a new one instead.
set -u

root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)}"
file="$(jq -r '.tool_input.file_path // empty')"
[ -n "$file" ] || exit 0
rel="${file#"$root"/}"
base="$(basename "$file")"

deny() {
  jq -n --arg r "$1" '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "deny", permissionDecisionReason: $r}}'
  exit 0
}

case "$base" in
  .env.example) ;;
  .env|.env.*) deny "$rel holds local secrets and is protected. Change .env.example instead, or ask the user to edit $rel." ;;
  Cargo.lock|pnpm-lock.yaml) deny "$rel is a lock file. Change it through cargo or pnpm, not by editing it." ;;
esac

case "$rel" in
  backend/migrations/*.sql)
    for ref in origin/master master; do
      if git -C "$root" cat-file -e "$ref:$rel" 2>/dev/null; then
        deny "$rel is already on $ref, so it has been applied. Editing it breaks the SQLx checksum. Add a new numbered migration instead."
      fi
    done
    ;;
esac
exit 0
