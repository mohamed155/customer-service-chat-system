#!/usr/bin/env bash
# Watch one PR until CI finishes AND Kilo Code AND CodeRabbit have reviewed the current head commit.
# Prints one line per event, then a final "DONE ..." line and exits.
#
# Usage: wait-for-pr.sh <pr-number> [kilo-timeout-min=30] [poll-seconds=45] [coderabbit-timeout-min=30]
#
# Events:
#   CHECK <name>: <pass|fail|skipping|cancel>
#   KILO <review|inline|comment> <id>@<version> <first line of body>
#   CODERABBIT <review|inline|comment> <id>@<version> <first line of body>
#   CODERABBIT <trigger posted|trigger already requested|skipped: ...|not requested ...>
#   DONE ci=<pass|fail> kilo=<reviewed|skipped|timeout> coderabbit=<reviewed|timeout|rate-limited|failed|skipped|off> head=<sha>
#   ERROR <message>
#
# Kilo's signals, in order of preference:
#   1. Its "Kilo Code Review" status check on the head commit finishing.
#   2. Kilo comments created OR edited after the head commit. Kilo keeps one summary comment
#      (marked <!-- kilo-review -->) and edits it in place on later pushes, so created_at
#      alone would miss re-reviews; updated_at catches them.
#
# CodeRabbit on this repo (fewer than 10 stars) never reviews automatically: its "CodeRabbit"
# check stays "skipping" ("manual review required") until someone comments "@coderabbitai review".
# So the watcher posts that comment once per run (CR_TRIGGER=0 disables; CR_FORCE_TRIGGER=1 posts it
# again even if a trigger comment already exists, e.g. after a rate limit). A review then takes
# about 5-10 minutes, and the plan allows 1 included review per hour. The "CodeRabbit" check is
# kept out of the ci=pass|fail result and reported separately as coderabbit=...:
#   reviewed      the CodeRabbit check finished successfully (read its review and inline comments)
#   rate-limited  CodeRabbit answered with a rate-limit notice instead of reviewing
#   failed        the CodeRabbit check itself failed or was cancelled, or the trigger could not be posted
#   skipped       it skipped the PR for a reason other than "manual review", or CR_TRIGGER=0
#   off           no CodeRabbit check ever appeared (the app is not installed on the repo)
#   timeout       no result within coderabbit-timeout-min
set -u
pr="$1"; kilo_timeout_min="${2:-30}"; poll="${3:-45}"; cr_timeout_min="${4:-30}"
repo=$(gh repo view --json nameWithOwner -q .nameWithOwner) || { echo "ERROR cannot resolve repo"; exit 1; }

head=$(gh pr view "$pr" --json headRefOid -q .headRefOid) || { echo "ERROR cannot read PR $pr"; exit 1; }
# Feedback only counts if it was posted or edited after the head commit was pushed.
since=$(gh api "repos/$repo/commits/$head" -q .commit.committer.date) || since="1970-01-01T00:00:00Z"
start=$(date +%s)
seen_checks=""; seen_kilo=""; seen_cr=""; kilo_seen=0; ci=""
cr_triggered=0; cr_start=$start; cr_since="$since"; cr_final=""

# Only feedback from the review GitHub Apps counts. Match the exact bot login AND require a Bot
# account, so a human user named e.g. "kilo-fan" can't inject review text into the pipeline.
# Confirmed: "kilo-code-bot[bot]" and "coderabbitai[bot]", both type "Bot". Override with
# KILO_LOGIN / CR_LOGIN if the apps are ever renamed.
KILO_LOGIN="${KILO_LOGIN:-kilo-code-bot[bot]}"
CR_LOGIN="${CR_LOGIN:-coderabbitai[bot]}"
CR_TRIGGER="${CR_TRIGGER:-1}"
CR_FORCE_TRIGGER="${CR_FORCE_TRIGGER:-0}"
is_kilo="(.user.login == \"$KILO_LOGIN\" and .user.type == \"Bot\")"
is_cr="(.user.login == \"$CR_LOGIN\" and .user.type == \"Bot\")"
# First non-empty line of a body that is not an HTML marker comment.
first_line='def first: (.body // "" | split("\n") | map(select(length>0 and (startswith("<!--")|not))) | .[0] // "");'

bot_items() {
  # $1 = jq condition selecting the bot.
  # One line per item: "<kind> <id>@<version> <first line>". The version changes when the bot
  # edits the item, so an edited summary is reported again.
  local who="$1"
  {
    gh api --paginate "repos/$repo/pulls/$pr/reviews" \
      --jq "$first_line .[] | select($who) | select(.submitted_at > \"$since\") | \"review \(.id)@\(.submitted_at) \(.state): \(first)\"" 2>/dev/null
    gh api --paginate "repos/$repo/pulls/$pr/comments" \
      --jq "$first_line .[] | select($who) | select(.updated_at > \"$since\") | \"inline \(.id)@\(.updated_at) \(.path):\(.line // .original_line) \(first)\"" 2>/dev/null
    gh api --paginate "repos/$repo/issues/$pr/comments" \
      --jq "$first_line .[] | select($who) | select(.updated_at > \"$since\") | \"comment \(.id)@\(.updated_at) \(first)\"" 2>/dev/null
  } | sed 's/<[^>]*>//g' | cut -c1-240
}

emit_new() {
  # $1 = KILO|CODERABBIT, $2 = jq bot condition. Seen-sets are kept per bot.
  local tag="$1" who="$2" k key
  while IFS= read -r k; do
    [ -z "$k" ] && continue
    key=$(cut -d' ' -f1-2 <<<"$k")
    if [ "$tag" = KILO ]; then
      grep -qxF "$key" <<<"$seen_kilo" || { echo "KILO $k"; seen_kilo+="$key"$'\n'; kilo_seen=1; }
    else
      grep -qxF "$key" <<<"$seen_cr" || { echo "CODERABBIT $k"; seen_cr+="$key"$'\n'; }
    fi
  done < <(bot_items "$who")
}

cr_bucket() {
  gh pr checks "$pr" --json name,bucket --jq '[.[] | select(.name == "CodeRabbit")][0].bucket // "none"' 2>/dev/null || echo none
}

cr_skip_reason() {
  gh api "repos/$repo/commits/$head/check-runs" --jq '[.check_runs[] | select(.name == "CodeRabbit")][0].output.title // ""' 2>/dev/null || true
}

cr_trigger_requested() {
  # A human (or an earlier run of this watcher) already asked for a review of this head?
  SINCE="$since" gh api --paginate "repos/$repo/issues/$pr/comments" \
    --jq '.[] | select(.user.type != "Bot") | select(.created_at > env.SINCE) | select(.body | test("@coderabbitai\\s+(full\\s+)?review"; "i")) | .id' 2>/dev/null | grep -q .
}

cr_rate_limit_notice() {
  # Succeeds (and prints the wait time) when CodeRabbit answered the trigger with a limit notice.
  # CodeRabbit posts two comments: "Review limit reached ... Next included review available in
  # N minutes" (about 5k characters of markup, so no length filter) and "Review rate limited".
  # Match those exact phrases, not the words "rate limit": a walkthrough of a PR that touches rate
  # limiting code would match those.
  local bodies wait
  bodies=$(CRSINCE="$cr_since" gh api --paginate "repos/$repo/issues/$pr/comments" \
    --jq ".[] | select($is_cr) | select(.updated_at > env.CRSINCE) | .body" 2>/dev/null | sed 's/<[^>]*>//g')
  grep -qiE 'review limit reached|review rate limited' <<<"$bodies" || return 1
  wait=$(grep -ioE 'next included review available in [^.*]*' <<<"$bodies" | head -1)
  echo "${wait:-wait time not stated}"
}

while true; do
  ci=""   # recomputed every poll: a check that registers late must flip it back to pending
  # --- CI (includes the Kilo Code Review check, except a Kilo run that hit its own limit; CodeRabbit
  # is handled separately below) ---
  # gh's built-in --jq is used so the script doesn't need a separate jq install.
  # `gh pr checks` exits non-zero while checks are pending/failing, so ignore its status.
  done_lines=$(gh pr checks "$pr" --json name,bucket \
    --jq '.[] | select(.bucket!="pending") | "\(.name): \(.bucket)"' 2>/dev/null | sort)
  while IFS= read -r l; do
    [ -n "$l" ] && ! grep -qxF "$l" <<<"$seen_checks" && { echo "CHECK $l"; seen_checks+="$l"$'\n'; }
  done <<<"$done_lines"
  # Summary, without CodeRabbit: "<total> <pending> <failed> <kilo checks> <kilo checks pending>"
  read -r total pending failed kilo_checks kilo_pending kilo_failed < <(gh pr checks "$pr" --json name,bucket \
    --jq '[.[] | select(.name != "CodeRabbit")] as $c | "\($c|length) \([$c[]|select(.bucket=="pending")]|length) \([$c[]|select(.bucket=="fail" or .bucket=="cancel")]|length) \([$c[]|select(.name|test("^Kilo Code";"i"))]|length) \([$c[]|select((.name|test("^Kilo Code";"i")) and .bucket=="pending")]|length) \([$c[]|select((.name|test("^Kilo Code";"i")) and (.bucket=="fail" or .bucket=="cancel"))]|length)"' 2>/dev/null || echo "0 0 0 0 0 0")
  # A Kilo run that ends with "the model reached its output limit" / "Review failed." produced no
  # review: that is Kilo's own limit, not a finding and not a CI failure. It is skipped (the user's
  # decision): no retry, no question, and it doesn't turn ci= into fail. A Kilo check that failed
  # because it found issues has other output and stays a failure.
  kilo_skipped=0
  if [ "${kilo_failed:-0}" -gt 0 ]; then
    limited=$(gh api "repos/$repo/commits/$head/check-runs" --jq '[.check_runs[] | select(.name|test("^Kilo Code";"i")) | select(.conclusion=="failure") | ((.output.title // "") + " " + (.output.summary // ""))] | map(select(test("output limit|did not finish|review failed";"i"))) | length' 2>/dev/null || echo 0)
    if [ "${limited:-0}" -gt 0 ] && [ "$limited" -ge "$kilo_failed" ]; then
      kilo_skipped=1
      failed=$(( failed - kilo_failed ))
    fi
  fi
  if [ "${total:-0}" -gt 0 ] && [ "${pending:-1}" -eq 0 ]; then
    if [ "${failed:-0}" -gt 0 ]; then ci=fail; else ci=pass; fi
  fi
  now=$(date +%s); elapsed_sec=$(( now - start ))

  # --- Kilo ---
  emit_new KILO "$is_kilo"
  kilo_result=""
  if [ "$kilo_skipped" = 1 ] && [ "${kilo_pending:-1}" -eq 0 ]; then
    kilo_result=skipped
  elif { [ "${kilo_checks:-0}" -gt 0 ] && [ "${kilo_pending:-1}" -eq 0 ]; } || [ "$kilo_seen" = 1 ]; then
    kilo_result=reviewed
  elif [ $(( elapsed_sec / 60 )) -ge "$kilo_timeout_min" ]; then
    kilo_result=timeout
  fi

  # --- CodeRabbit ---
  emit_new CODERABBIT "$is_cr"
  # Once CodeRabbit has a result for this commit it is final: don't re-evaluate (or re-print) it.
  cr_result="$cr_final"
  if [ -z "$cr_result" ]; then
  bucket=$(cr_bucket)
  case "$bucket" in
    pass) cr_result=reviewed ;;
    fail|cancel) cr_result=failed ;;
    skipping|pending)
      if [ "$bucket" = skipping ] && [ "$cr_triggered" = 0 ]; then
        reason=$(cr_skip_reason)
        if [ -n "$reason" ] && ! grep -qi 'manual review' <<<"$reason"; then
          echo "CODERABBIT skipped: $reason"; cr_result=skipped
        elif [ "$CR_TRIGGER" != 1 ]; then
          echo "CODERABBIT not requested (CR_TRIGGER=0): $reason"; cr_result=skipped
        elif [ "$CR_FORCE_TRIGGER" != 1 ] && cr_trigger_requested; then
          echo "CODERABBIT trigger already requested for this commit"
          cr_triggered=1; cr_start=$now
        elif gh pr comment "$pr" --body "@coderabbitai review" >/dev/null 2>&1; then
          echo "CODERABBIT trigger posted: @coderabbitai review"
          cr_triggered=1; cr_start=$now; cr_since=$(date -u +%Y-%m-%dT%H:%M:%SZ)
        else
          echo "CODERABBIT could not post the trigger comment"; cr_result=failed
        fi
      elif [ "$cr_triggered" = 1 ] && notice=$(cr_rate_limit_notice); then
        echo "CODERABBIT rate-limited: $notice"
        cr_result=rate-limited
      elif [ $(( (now - cr_start) / 60 )) -ge "$cr_timeout_min" ]; then
        cr_result=timeout   # triggered but never finished, or someone else started a review that never finished
      fi ;;
    none)
      # The check can take a moment to register after a push; no check at all means no app.
      [ "$elapsed_sec" -ge 180 ] && cr_result=off ;;
  esac
  cr_final="$cr_result"
  fi

  # Workflows can take a little while to register their checks after a push. Never report DONE
  # in the first 2 minutes, so a poll that only sees the fast checks can't look like "all green".
  [ "$elapsed_sec" -lt 120 ] && ci=""
  if [ -n "$ci" ] && [ -n "$kilo_result" ] && [ -n "$cr_result" ]; then
    sleep "$poll"   # let the reviewers finish posting/editing comments after their checks complete
    emit_new KILO "$is_kilo"; emit_new CODERABBIT "$is_cr"
    echo "DONE ci=$ci kilo=$kilo_result coderabbit=$cr_result head=$head"; exit 0
  fi
  sleep "$poll"
done
