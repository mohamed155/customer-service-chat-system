# Load Tests

k6 scripts for measuring platform capacity under load (FR-041, FR-042, SC-014).

## Scenarios

| Script | Path | What it exercises |
|---|---|---|
| `dashboard-read.js` | `loadtest/dashboard-read.js` | Authenticated dashboard reads: conversations list, analytics summary, knowledge base listing |
| `widget-conversation.js` | `loadtest/widget-conversation.js` | Widget session creation and message sending |
| `webhook-ingest.js` | `loadtest/webhook-ingest.js` | Signed inbound webhook delivery (WhatsApp format) |

## Concurrency & Latency Targets

| Scenario | Target concurrency (VUs) | Error rate | p95 latency |
|---|---|---|---|
| Dashboard reads | 200 | < 1% | < 500 ms |
| Widget conversation | 100 | < 1% | < 500 ms |
| Webhook ingest | 80 | < 1% | < 500 ms |

These targets represent the platform's documented capacity evidence (FR-042). The
concurrency levels reflect the relative traffic mix expected in production: operators view
dashboards more frequently than customers start new widget sessions, and webhook bursts
are the least concurrent path. Adjust values in the `options.stages` blocks of each `.js`
file as capacity data accumulates from staging runs.

## Prerequisites

- [k6](https://k6.io/docs/getting-started/installation/) installed locally
- A staging environment or a local backend with `APP_RATE_LIMIT_ENABLED=false` (for
  `widget-conversation.js`, so the rate limiter from US1 does not refuse the load and
  distort capacity measurements)
- `K6_BASE_URL` environment variable pointing at the target environment

## Safety Guards (FR-043)

Every script refuses to run unless:

1. **`K6_BASE_URL` is set** — no default fallback. The script exits with code 2 if unset.
2. **Host matches `LOADTEST_ALLOWED_HOSTS`** — a regex pattern validated against the URL's
   hostname. Set it to guard against accidental misconfiguration.

```bash
export LOADTEST_ALLOWED_HOSTS='\.staging\.example\.com$|^localhost(:|$)'
export K6_BASE_URL=https://staging.example.com
```

## Running

```bash
# Single VU / short duration (smoke test)
k6 run --vus 1 --duration 5s loadtest/dashboard-read.js

# Full load (staging only)
k6 run loadtest/dashboard-read.js
k6 run loadtest/widget-conversation.js
k6 run loadtest/webhook-ingest.js
```

### Widget script note

Set `APP_RATE_LIMIT_ENABLED=false` on the target before running
`widget-conversation.js`. The rate limiter from US1 would otherwise throttle the
artificial load before capacity is reached, measuring the limiter rather than the
platform.

```bash
# Confirm rate limiting is really off: 15 rapid session creations, zero 429s.
# (The widget session budget is 10 per minute, so a limited target WILL 429 here.)
for i in $(seq 1 15); do
  curl -s -o /dev/null -w '%{http_code}\n' -X POST "$K6_BASE_URL/widget/v1/sessions" \
    -H 'content-type: application/json' -d '{}'
done | sort | uniq -c
# Expected: no 429 line. If 429 appears, APP_RATE_LIMIT_ENABLED is not false on the
# target and the run would measure the limiter, not capacity (FR-043).
```

## Recording Results as Capacity Evidence (FR-042)

Each k6 run produces a summary with:
- `http_req_duration` — median, p(95), p(99)
- `http_req_failed` — percentage
- `iterations` — total completed

To record results for capacity evidence:

1. **Save the full output** to a file:
   ```bash
   k6 run loadtest/dashboard-read.js 2>&1 | tee loadtest/results/$(date -u +%FT%H%M%S)-dashboard-read.txt
   ```

2. **Capture the JSON summary** for machine parsing:
   ```bash
   k6 run --summary-export=loadtest/results/$(date -u +%FT%H%M%S)-dashboard-read.json loadtest/dashboard-read.js
   ```

3. **Include the following in each evidence record:**
   - Date and environment (staging hostname, commit SHA)
   - Script and stage configuration (VUs, duration)
   - Throughput (`iterations` / duration)
   - Latency distribution (median, p(95), p(99))
   - Error rate (`http_req_failed`)
   - Any anomalies (e.g., Redis degradation, connection timeouts)

4. **Baseline comparison**: Keep the most recent successful run for each scenario and
   compare against it on subsequent runs to detect regressions.

Results go in `loadtest/results/` (not committed — generated artifacts). The
`results/.gitkeep` file tracks the directory.
