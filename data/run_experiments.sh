#!/usr/bin/env bash
set -eo pipefail

main() {
  TMP_DIR=$(mktemp -d)
  echo "Temporary directory:" ${TMP_DIR}
  mkdir ${TMP_DIR}/{experiments,results}

  # Need to be able to run "python -m experiments"
  cd $(git rev-parse --show-toplevel) > /dev/null

  # Make experiment spec files
  pushd data > /dev/null
  python make_experiments.py ${TMP_DIR}/experiments
  popd > /dev/null

  for system in express riposte spectrum; do
    for exp_path in ${TMP_DIR}/experiments/*${system}*.json; do
      exp=$(basename "$exp_path")
      if [ ! -z ${1+nonempty} ] && [[ ! "$exp" =~ "${1}" ]]; then
        # a filter was given, but it doesn't match this experiment
        continue
      else
        echo "Running ${exp}"
        declare "ran_${system}=1"  # so we clean up later
        if [ ${system} == "spectrum" and ! -z ${COMMIT+nonempty} ]; then
          extra_args="--commit ${COMMIT}"
        else
          extra_args=""
        fi
        python -m experiments \
          --output ${TMP_DIR}/results/${exp} \
          ${system} ${extra_args} ${exp_path}
      fi
    done

    # Clean up AWS resources
    var="ran_${system}"
    if [ ! -z ${!var} ]; then
      echo "[]" | python -m experiments --cleanup ${system} -
    fi
  done

  if [ ! -z ${RESULTS_DIR} ]; then
    cp -r ${TMP_DIR}/results ${RESULTS_DIR}
  else
    echo "Pausing (good time to copy out ${TMP_DIR}/results/*)..."
    read
  fi

  rm -rf ${TMP_DIR}
  exit
}

main $@
