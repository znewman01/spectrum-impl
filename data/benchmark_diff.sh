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
  RESULTS_DIR=/home/zjn/tmp/results  # TODO: make argument

  # Where the benchmark spec json files will live
  BENCH_SPEC_DIR=$(mktemp -d)
}

main() {
    parse_args $@

    # 1. Get benchmark data for both commits!
    # This is a pretty complicated step; need to:
    # - generate the spec for benchmarks
    # - run the benchmarks (in AWS) for uncached values
    # - clean up AWS

    # Generate full set of experiments
    data_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
    python "$data_dir/make_experiments.py" "$BENCH_SPEC_DIR"

    # Set up so we can run the experiments script
    GIT_ROOT=$(git rev-parse --show-toplevel)
    export PYTHONPATH="${GIT_ROOT}:${PYTHONPATH}"
    CLEANUP_NEEDED=0

    mkdir -p "${RESULTS_DIR}"
    for bench_path in $(find "${BENCH_SPEC_DIR}/" -name "benchmarks-*.json"); do
        bench_name=$(basename "${bench_path}")

        for commit in "${COMMIT1}" "${COMMIT2}"; do
            results_name="${commit}-${bench_name}"
            results_path="${RESULTS_DIR}/${results_name}"
            if  [ -f "${results_path}" ]; then
                echo "${results_name} exists in ${RESULTS_DIR}; skipping..."
            else
                # The "|| true" is there to make sure we still clean up, nothing
                # worse than leaving resources dangling.
                CLEANUP_NEEDED=1
                python -m experiments \
                    --commit "$commit" \
                    --output "${results_path}" \
                    spectrum --build release "${bench_path}" || true
            fi
        done
    done

    # Clean up iff we ran any experiments
    if [ "${CLEANUP_NEEDED}" -eq 1 ]; then
       echo "[]" | python -m experiments --cleanup spectrum -  # magic invocation for "tear down AWS"
    fi

    rm "${BENCH_SPEC_DIR}"/*.json
    rmdir "${BENCH_SPEC_DIR}"

    # 2. Plot
    python data/plot_benchmark_diff.py \
        --results-dir "${RESULTS_DIR}" \
        --baseline "${COMMIT1}" \
        --new "${COMMIT2}" \
        --output "${RESULTS_DIR}/${COMMIT1}-${COMMIT2}.png"
}

main $@
