#!/usr/bin/env bash
set -eo pipefail

main() {
  TMP_DIR=$(mktemp -d)
  echo "Temporary directory:" ${TMP_DIR}
  mkdir ${TMP_DIR}/{experiments,results}

  # Need to be able to run "python -m experiments"
  pushd $(git rev-parse --show-toplevel) > /dev/null

  # Make experiment spec files
  pushd data > /dev/null
  python make_experiments.py ${TMP_DIR}/experiments
  popd > /dev/null

  for system in express riposte spectrum; do
    for exp_path in ${TMP_DIR}/experiments/*${system}*.json; do
      exp=$(basename "$exp_path")
      if [ ! -z ${1+nonempty} ] && [[ ! "$exp" =~ "${1}" ]]; then
        # if a filter was given, but it doesn't match this experiment
        continue
      else
        echo "Running ${exp}"
        declare "ran_${system}=1"
        python -m experiments --output ${TMP_DIR}/results/${exp} ${system} ${exp_path}
      fi
    done
  done

  # TODO: copy out results

  # Clean up AWS resources
  for system in express riposte spectrum; do
    var="ran_${system}"
    if [ ! -z ${!var} ]; then
      echo "[]" | python -m experiments --cleanup express -
    fi
  done

  echo "Pausing (good time to copy out ${TMP_DIR}/results/*)..."
  read
  rm -rf ${TMP_DIR}

  popd > /dev/null
  exit
}

main $@
