#!/usr/bin/env bash
# Watch one PR until CI finishes AND Kilo Code has reviewed the current head commit.
# Prints one line per event, then a final "DONE ..." line and exits.
#
# Usage: wait-for-pr.sh <pr-number> [kilo-timeout-minutes=30] [poll-seconds=45]
#
# Events:
#   CHECK <name>: <pass|fail|skipping|cancel>
#   KILO <review|inline|comment> <id> <first line of body>
#   DONE ci=<pass|fail> kilo=<reviewed|timeout> head=<sha>
#   ERROR <message>
#
# Kilo's signals, in order of preference:
#   1. Its "Kilo Code Review" status check on the head commit finishing. Kilo has finished
#      reviewing once that check is no longer pending.
#   2. Kilo comments created OR edited after the head commit. Kilo keeps one summary comment
#      (marked <!-- kilo-review -->) and edits it in place on later pushes, so created_at
#      alone would miss re-reviews; updated_at catches them.
set -u
pr="$1"; kilo_timeout_min="${2:-30}"; poll="${3:-45}"
repo=$(gh repo view --json nameWithOwner -q .nameWithOwner) || { echo "ERROR cannot resolve repo"; exit 1; }

head=$(gh pr view "$pr" --json headRefOid -q .headRefOid) || { echo "ERROR cannot read PR $pr"; exit 1; }
# Kilo feedback only counts if it was posted or edited after the head commit was pushed.
since=$(gh api "repos/$repo/commits/$head" -q .commit.committer.date) || since="1970-01-01T00:00:00Z"
start=$(date +%s)
seen_checks=""; seen_kilo=""; kilo_seen=0; ci=""

# Only feedback from the Kilo Code GitHub App counts. Match the exact bot login AND require a
# Bot account, so a human user named e.g. "kilo-fan" can't inject review text into the pipeline.
# Confirmed on PR #9: login "kilo-code-bot[bot]", type "Bot". Override with KILO_LOGIN if needed.
KILO_LOGIN="${KILO_LOGIN:-kilo-code-bot[bot]}"
is_kilo="(.user.login == \"$KILO_LOGIN\" and .user.type == \"Bot\")"

kilo_items() {
  # One line per Kilo item: "<kind> <id>@<version> <body first line>". The version changes when
  # Kilo edits the item, so an edited summary is reported again.
  {
    gh api --paginate "repos/$repo/pulls/$pr/reviews" \
      --jq ".[] | select($is_kilo) | select(.submitted_at > \"$since\") | \"review \(.id)@\(.submitted_at) \(.state): \(.body|split(\"\n\")[0])\"" 2>/dev/null
    gh api --paginate "repos/$repo/pulls/$pr/comments" \
      --jq ".[] | select($is_kilo) | select(.updated_at > \"$since\") | \"inline \(.id)@\(.updated_at) \(.path):\(.line // .original_line) \(.body|split(\"\n\")[0])\"" 2>/dev/null
    gh api --paginate "repos/$repo/issues/$pr/comments" \
      --jq ".[] | select($is_kilo) | select(.updated_at > \"$since\") | \"comment \(.id)@\(.updated_at) \(.body|split(\"\n\")[0])\"" 2>/dev/null
  } | cut -c1-240
}

emit_new_kilo() {
  while IFS= read -r k; do
    key=$(cut -d' ' -f1-2 <<<"$k")
    [ -n "$k" ] && ! grep -qxF "$key" <<<"$seen_kilo" && { echo "KILO $k"; seen_kilo+="$key"$'\n'; kilo_seen=1; }
  done < <(kilo_items)
}

while true; do
  ci=""   # recomputed every poll: a check that registers late must flip it back to pending
  # --- CI (includes the Kilo Code Review check) ---
  # gh's built-in --jq is used so the script doesn't need a separate jq install.
  # `gh pr checks` exits non-zero while checks are pending/failing, so ignore its status.
  done_lines=$(gh pr checks "$pr" --json name,bucket \
    --jq '.[] | select(.bucket!="pending") | "\(.name): \(.bucket)"' 2>/dev/null | sort)
  while IFS= read -r l; do
    [ -n "$l" ] && ! grep -qxF "$l" <<<"$seen_checks" && { echo "CHECK $l"; seen_checks+="$l"$'\n'; }
  done <<<"$done_lines"
  # Summary: "<total> <pending> <failed> <kilo checks> <kilo checks pending>"
  read -r total pending failed kilo_checks kilo_pending < <(gh pr checks "$pr" --json name,bucket \
    --jq '"\(length) \([.[]|select(.bucket=="pending")]|length) \([.[]|select(.bucket=="fail" or .bucket=="cancel")]|length) \([.[]|select(.name|test("^Kilo Code";"i"))]|length) \([.[]|select((.name|test("^Kilo Code";"i")) and .bucket=="pending")]|length)"' 2>/dev/null || echo "0 0 0 0 0")
  if [ "${total:-0}" -gt 0 ] && [ "${pending:-1}" -eq 0 ]; then
    if [ "${failed:-0}" -gt 0 ]; then ci=fail; else ci=pass; fi
  fi

  # --- Kilo ---
  emit_new_kilo
  kilo_check_done=0
  [ "${kilo_checks:-0}" -gt 0 ] && [ "${kilo_pending:-1}" -eq 0 ] && kilo_check_done=1

  elapsed_sec=$(( $(date +%s) - start )); elapsed_min=$(( elapsed_sec / 60 ))
  # Workflows can take a little while to register their checks after a push. Never report DONE
  # in the first 2 minutes, so a poll that only sees the fast checks can't look like "all green".
  [ "$elapsed_sec" -lt 120 ] && ci=""
  if [ -n "$ci" ]; then
    if [ "$kilo_check_done" = 1 ] || [ "$kilo_seen" = 1 ]; then
      sleep "$poll"   # let Kilo finish posting/editing comments after its check completes
      emit_new_kilo
      echo "DONE ci=$ci kilo=reviewed head=$head"; exit 0
    fi
    if [ "$elapsed_min" -ge "$kilo_timeout_min" ]; then
      echo "DONE ci=$ci kilo=timeout head=$head"; exit 0
    fi
  fi
  sleep "$poll"
done
