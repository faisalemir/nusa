#!/usr/bin/env sh
# Extract P50, P99, and RPS from wrk stdout. Usage: parse-wrk-percentiles.sh < wrk.out
set -eu
p50=""
p99=""
rps=""
while IFS= read -r line; do
  case "$line" in
    *"50.000%"*|*"     50%"*)
      p50=$(echo "$line" | awk '{print $2}')
      ;;
    *"99.000%"*|*"     99%"*)
      p99=$(echo "$line" | awk '{print $2}')
      ;;
    *"Requests/sec"*)
      rps=$(echo "$line" | awk '{print $2}')
      ;;
  esac
  # wrk 4.x without HdrHistogram: Thread Stats Latency avg (col 2) and max (col 4)
  case "$line" in
    *"Latency"*)
      if [ -z "$p50" ]; then
        p50=$(echo "$line" | awk '{print $2}')
        p99=$(echo "$line" | awk '{print $4}')
      fi
      ;;
  esac
done
printf 'p50=%s p99=%s rps=%s\n' "${p50:-n/a}" "${p99:-n/a}" "${rps:-n/a}"
