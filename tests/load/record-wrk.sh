#!/bin/sh
# P3: capture wrk output for docs/benchmarks/normal-mode-report.md
# Usage: ./tests/load/record-wrk.sh http://127.0.0.1:8080/ [label]
set -eu
URL="${1:-http://127.0.0.1:8080/}"
LABEL="${2:-nusa}"
OUT="${3:-/tmp/nusa-wrk-${LABEL}.txt}"
THREADS="${WRK_THREADS:-4}"
CONNECTIONS="${WRK_CONNECTIONS:-100}"
DURATION="${WRK_DURATION:-30s}"

if ! command -v wrk >/dev/null 2>&1; then
  echo "wrk not installed. On Alpine: apk add wrk" >&2
  exit 1
fi

{
  echo "# wrk ${LABEL} $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "# url=${URL} threads=${THREADS} connections=${CONNECTIONS} duration=${DURATION}"
  wrk -t"${THREADS}" -c"${CONNECTIONS}" -d"${DURATION}" "${URL}"
} | tee "${OUT}"
echo "Wrote ${OUT}"
