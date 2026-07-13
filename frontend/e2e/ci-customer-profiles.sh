#!/usr/bin/env bash
set -euo pipefail

export CI_REAL_BACKEND=true

cd "$(dirname "$0")/.."

npx playwright test --config=playwright.real.config.ts
