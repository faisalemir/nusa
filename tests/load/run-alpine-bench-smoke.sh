#!/usr/bin/env sh
# P3 smoke: short wrk runs against Nusa child + Octane fixtures in Alpine.
# Not a GA sign-off — records reproducible numbers in docs/benchmarks/artifacts/.
set -eu

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# Nested Podman cannot always install a process-wide seccomp filter; landlock still applies.
export NUSA_SKIP_SECCOMP="${NUSA_SKIP_SECCOMP:-1}"

cp -f .cargo/config-alpine.toml .cargo/config.toml
mkdir -p /tmp/nusa-bench docs/benchmarks/artifacts
chmod 1777 /tmp/nusa-bench 2>/dev/null || true

if ! command -v wrk >/dev/null 2>&1; then
  apk add --no-cache wrk curl || {
    alpine_ver=$(cut -d. -f1,2 /etc/alpine-release)
    apk add --no-cache --repository="https://dl-cdn.alpinelinux.org/alpine/v${alpine_ver}/community" wrk curl
  }
fi
if ! command -v wrk >/dev/null 2>&1; then
  echo "wrk not available; rebuild image: just podman-build (includes wrk)" >&2
  exit 1
fi
if ! command -v curl >/dev/null 2>&1; then
  apk add --no-cache curl
fi

if [ "${NUSA_BENCH_RELEASE:-0}" = "1" ]; then
  echo "Building nusa (release)..."
  cargo build --release -p nusa-cli
  BIN="${CARGO_TARGET_DIR:-/opt/nusa-target}/release/nusa"
else
  echo "Building nusa (dev profile — set NUSA_BENCH_RELEASE=1 for GA numbers)..."
  cargo build -p nusa-cli
  BIN="${CARGO_TARGET_DIR:-/opt/nusa-target}/debug/nusa"
fi
test -x "$BIN" || {
  echo "nusa binary missing at $BIN" >&2
  exit 1
}

wait_ready() {
  url="$1"
  i=0
  while [ "$i" -lt 90 ]; do
    code=$(curl -s -o /dev/null -w '%{http_code}' "$url" 2>/dev/null || echo "000")
    if [ "$code" = "200" ]; then
      return 0
    fi
    i=$((i + 1))
    sleep 0.5
  done
  echo "timeout waiting for HTTP 200 from $url (last code=${code:-unknown})" >&2
  return 1
}

run_wrk() {
  label="$1"
  url="$2"
  out="docs/benchmarks/artifacts/wrk-${label}-$(date -u +%Y%m%d).txt"
  WRK_DURATION="${WRK_DURATION:-5s}" \
  WRK_CONNECTIONS="${WRK_CONNECTIONS:-10}" \
  WRK_THREADS="${WRK_THREADS:-2}" \
    sh tests/load/record-wrk.sh "$url" "$label" "$out"
  if grep -q "Non-2xx or 3xx responses:" "$out"; then
  non2xx=$(grep "Non-2xx or 3xx responses:" "$out" | awk '{print $NF}')
    if [ -n "$non2xx" ] && [ "$non2xx" != "0" ]; then
      echo "WARN: ${label} had ${non2xx} non-2xx responses — raise max_workers or lower WRK_CONNECTIONS" >&2
    fi
  fi
  stats=$(sh tests/load/parse-wrk-percentiles.sh <"$out")
  echo "${label}: ${stats} (see ${out})"
  eval "$stats"
}

stop_server() {
  if [ -n "${NUSA_PID:-}" ] && kill -0 "$NUSA_PID" 2>/dev/null; then
    kill "$NUSA_PID" 2>/dev/null || true
    wait "$NUSA_PID" 2>/dev/null || true
  fi
  NUSA_PID=""
}

# --- Child engine (php-static-minimal) ---
"$BIN" --config /src/tests/load/nusa-bench-child.toml >/tmp/nusa-bench-child.log 2>&1 &
NUSA_PID=$!
trap stop_server EXIT
wait_ready "http://127.0.0.1:18080/health"
run_wrk "child-root" "http://127.0.0.1:18080/"
CHILD_P50="$p50"
CHILD_P99="$p99"
CHILD_RPS="$rps"
stop_server
trap - EXIT

# --- Octane (Laravel fixture) ---
sh dockerfiles/podman-laravel-fixture.sh
"$BIN" --config /src/tests/load/nusa-bench-octane.toml >/tmp/nusa-bench-octane.log 2>&1 &
NUSA_PID=$!
trap stop_server EXIT
wait_ready "http://127.0.0.1:18081/ready"
# Warm one request so the Octane worker is idle before wrk
curl -sf "http://127.0.0.1:18081/nusa-ping" >/dev/null || {
  echo "Octane warm-up failed; log:" >&2
  tail -30 /tmp/nusa-bench-octane.log >&2
  exit 1
}
run_wrk "octane-ping" "http://127.0.0.1:18081/nusa-ping"
OCTANE_P50="$p50"
OCTANE_P99="$p99"
OCTANE_RPS="$rps"
stop_server

# --- Octane embed (stdio daemon, JSON transport) ---
sh dockerfiles/podman-laravel-fixture.sh
NUSA_EMBED_TRANSPORT=json "$BIN" --config /src/tests/load/nusa-bench-embed.toml >/tmp/nusa-bench-embed.log 2>&1 &
NUSA_PID=$!
trap stop_server EXIT
wait_ready "http://127.0.0.1:18082/ready"
curl -sf "http://127.0.0.1:18082/nusa-ping" >/dev/null || {
  echo "Embed warm-up failed; log:" >&2
  tail -30 /tmp/nusa-bench-embed.log >&2
  exit 1
}
run_wrk "embed-ping" "http://127.0.0.1:18082/nusa-ping"
EMBED_JSON_P50="$p50"
EMBED_JSON_P99="$p99"
EMBED_JSON_RPS="$rps"
stop_server
trap - EXIT

# --- Octane embed + NEB1 frame transport ---
sh dockerfiles/podman-laravel-fixture.sh
NUSA_EMBED_TRANSPORT=frame "$BIN" --config /src/tests/load/nusa-bench-embed.toml >/tmp/nusa-bench-embed-frame.log 2>&1 &
NUSA_PID=$!
trap stop_server EXIT
wait_ready "http://127.0.0.1:18082/ready"
curl -sf "http://127.0.0.1:18082/nusa-ping" >/dev/null || {
  echo "Embed frame warm-up failed; log:" >&2
  tail -30 /tmp/nusa-bench-embed-frame.log >&2
  exit 1
}
run_wrk "embed-frame-ping" "http://127.0.0.1:18082/nusa-ping"
EMBED_FRAME_P50="$p50"
EMBED_FRAME_P99="$p99"
EMBED_FRAME_RPS="$rps"
stop_server
trap - EXIT

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
DATE=$(date -u +%Y-%m-%d)
SUMMARY="docs/benchmarks/artifacts/normal-smoke-${DATE}.md"
cat >"$SUMMARY" <<EOF
# Normal mode wrk smoke (${DATE})

Git: \`${SHA}\` | Environment: Alpine musl (Podman) | Duration: ${WRK_DURATION:-5s} | Connections: ${WRK_CONNECTIONS:-10}

| Scenario | URL | P50 | P99 | RPS |
|----------|-----|-----|-----|-----|
| S1 child GET / | nusa-bench-child | ${CHILD_P50} | ${CHILD_P99} | ${CHILD_RPS} |
| S2 Octane IPC GET /nusa-ping | nusa-bench-octane | ${OCTANE_P50} | ${OCTANE_P99} | ${OCTANE_RPS} |
| S2-embed JSON GET /nusa-ping | nusa-bench-embed | ${EMBED_JSON_P50} | ${EMBED_JSON_P99} | ${EMBED_JSON_RPS} |
| S2-embed frame GET /nusa-ping | nusa-bench-embed + NEB1 | ${EMBED_FRAME_P50} | ${EMBED_FRAME_P99} | ${EMBED_FRAME_RPS} |

FPM baseline still required on the same host before GA — see [normal-mode-report.md](../normal-mode-report.md).
EOF

echo "Wrote ${SUMMARY}"
