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
  NEW_COMMIT=$(parse_commitish $1)
  BASELINE_COMMIT=$(parse_commitish $2)
  if [ "$NEW_COMMIT" == "$BASELINE_COMMIT" ]; then
      echo "Error: [${1}] and [${2}] resolve to the same commit: [${NEW_COMMIT}]" >&2
      exit 1
  fi

  # Cache results in this directory
  RESULTS_DIR=/home/zjn/tmp/bench  # TODO: make argument
}

main() {
    parse_args $@

    # 1. Get benchmark data for both commits!
    for COMMIT in $NEW_COMMIT $BASELINE_COMMIT; do
      COMMIT_DIR="${RESULTS_DIR}/${COMMIT}"
      echo "Processing commit $COMMIT"
      if [ -d "${COMMIT_DIR}" ]; then
        echo "Results exist (${COMMIT_DIR}), skipping"
        continue
      fi
      COMMIT=${COMMIT} RESULTS_DIR=${COMMIT_DIR} bash "data/run_experiments.sh" spectrum
    done

    # 2. Plot
    nix-shell /home/zjn/git/spectrum-paper/shell.nix --command \
      "python /home/zjn/git/spectrum-paper/experiments/plot.py \
          --results-dir ${RESULTS_DIR} \
          --benchmark ${BASELINE_COMMIT}:${NEW_COMMIT} \
          --show"
    exit 0
}

main $@
