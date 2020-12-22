#!/usr/bin/env bash
set -euo pipefail

display_usage() {
    echo "Usage: $0 NEW_COMMIT BASELINE_COMMIT"
    echo
    echo "Plot the performance difference between two commits."
}

parse_commitish() {
  if ! COMMIT=$(git rev-parse --verify --quiet "$1^{commit}"); then
      echo "Error: $1 is not a valid commit(ish)" >&2
      return 1
  fi
  echo ${COMMIT:0:6}
}

parse_args() {
  if [ $# -ne 2 ]; then
      display_usage
      exit 1
  fi
  COMMIT1=$(parse_commitish $1)
  COMMIT2=$(parse_commitish $2)
  if [ "$COMMIT1" == "$COMMIT2" ]; then
      echo "Error: [${1}] and [${2}] resolve to the same commit: [${COMMIT1}]" >&2
      exit 1
  fi

  # Cache results in this directory
  RESULTS_DIR=/home/zjn/tmp/bench  # TODO: make argument
}

main() {
    parse_args $@

    # 1. Get benchmark data for both commits!
    for COMMIT in $COMMIT1 $COMMIT2; do
      COMMIT_DIR="${RESULTS_DIR}/${COMMIT}"
      echo "Processing commit $COMMIT"
      if [ -d "${COMMIT_DIR}" ]; then
        echo "Results exist (${COMMIT_DIR}), skipping"
        continue
      fi
      COMMIT=${COMMIT} RESULTS_DIR=${COMMIT_DIR} bash "data/run_experiments.sh" spectrum
    done

    # 2. Plot
    python data/plot_benchmark_diff.py \
        --results-dir "${RESULTS_DIR}" \
        --new "${COMMIT1}" \
        --baseline "${COMMIT2}" \
        # --output "${RESULTS_DIR}/${COMMIT1}-${COMMIT2}.png"
    exit 0
}

main $@
