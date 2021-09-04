#!/usr/bin/env bash
set -eo pipefail

main() {
  TMP_DIR=$(mktemp -d)
  TRIALS=10
  echo "Temporary directory:" ${TMP_DIR}
  mkdir ${TMP_DIR}/{experiments,results}

  # Need to be able to run "python -m experiments"
  cd $(git rev-parse --show-toplevel) > /dev/null

  # Make experiment spec files
  pushd data > /dev/null
  python make_experiments.py --trials ${TRIALS} ${TMP_DIR}/experiments
  popd > /dev/null

  for system in express riposte spectrum; do
    for exp_path in ${TMP_DIR}/experiments/*${system}*.json; do
      exp=$(basename "$exp_path")
      any_match=0
      for filter in $@; do
        if [[ "$exp" =~ "${filter}" ]]; then
          # a filter was given and matches the experiment file
          any_match=1
          break
        fi
      done
      if [ $# -gt 0 ] && [ $any_match -eq 0 ]; then
        # filter(s) were provided, but none matched: skip!
        continue
      fi
      echo "Running ${exp}"
      declare "ran_${system}=1"  # so we clean up later
      if [ ${system} == "spectrum" ] && [ ! -z ${COMMIT+nonempty} ]; then
        extra_args="--commit ${COMMIT}"
      else
        extra_args=""
      fi
      python -m experiments \
        --output ${TMP_DIR}/results/${exp} \
        ${system} ${extra_args} ${exp_path} || true
    done

    # Extra data
    var="ran_${system}"
    if [ ! -z ${!var} ]; then
      if [ $system = "spectrum" ]; then
        # Local stuff: date, Rust version, LoC count
        date "+%B %Y" \
          > ${TMP_DIR}/results/experiment-date.txt
        rustc --version | grep -Eo '20[0-9]{2}-[0-9]{2}-[0-9]{2}' \
          > ${TMP_DIR}/results/rust-nightly-date.txt
        tokei --output json > ${TMP_DIR}/results/loc.json
        # AWS instance information
        python -m experiments.spectrum.ssh --worker -- \
          "ec2metadata | grep instance-type | sed 's/instance-type: //'" \
          > ${TMP_DIR}/results/instance-type.txt
        instance_type=$(cat ${TMP_DIR}/results/instance-type.txt  | tr -d '\n')
        nix-shell -p awscli2 \
          --pure \
          --keep AWS_ACCESS_KEY_ID \
          --keep AWS_SECRET_ACCESS_KEY \
          --command "aws ec2 --region us-east-2 describe-instance-types --instance-types=${instance_type}" \
          > ${TMP_DIR}/results/instance.json
        curl -sL "https://ec2.shop?filter=${instance_type}" -H 'accept: json' \
          > ${TMP_DIR}/results/instance-cost.json
        # Performance
        python -m experiments.spectrum.ssh --worker -- \
          "openssl speed -elapsed -evp aes-128-ctr 2>&1" \
          > ${TMP_DIR}/results/openssl-stderr.txt
        # Network
        python -m experiments.spectrum.ssh --worker -- \
          "ec2metadata | grep public-ip" \
          > ${TMP_DIR}/results/openssl-stderr.txt
        # python -m experiments.spectrum.ssh --worker
        W0_REGION=$(python -m experiments.spectrum.ssh --worker 0 -- "ec2metadata --availability-zone | grep -Eo 'us-[a-z]+'")
        W1_REGION=$(python -m experiments.spectrum.ssh --worker 1 -- "ec2metadata --availability-zone | grep -Eo 'us-[a-z]+'")
        if [ "$W0_REGION" != "$W1_REGION" ]; then
          C_IP=$(python -m experiments.spectrum.ssh --client -- "ec2metadata --public-ip")
          W0_IP=$(python -m experiments.spectrum.ssh --worker 0 -- "ec2metadata --public-ip")
          W1_IP=$(python -m experiments.spectrum.ssh --worker 1 -- "ec2metadata --public-ip")
          # grab all the ping times, sort numerically, get the middle => median
          PING_COUNT=15
          python -m experiments.spectrum.ssh --worker 0 -- \
            "ping -c $PING_COUNT $W1_IP" \
            | grep -Eo 'time=[0-9.]+ ms' \
            | sed -E 's/time=([0-9.]+) ms/\1/g' \
            | sort -n \
            | sed -n "$((($PING_COUNT + 1) / 2))p" \
            > ${TMP_DIR}/results/median-rtt-w0-w1.txt
          python -m experiments.spectrum.ssh --client -- \
            "ping -c $PING_COUNT $W0_IP" \
            | grep -Eo 'time=[0-9.]+ ms' \
            | sed -E 's/time=([0-9.]+) ms/\1/g' \
            | sort -n \
            | sed -n "$((($PING_COUNT + 1) / 2))p" \
            > ${TMP_DIR}/results/median-rtt-c-w0.txt
          python -m experiments.spectrum.ssh --client -- \
            "ping -c $PING_COUNT $W1_IP" \
            | grep -Eo 'time=[0-9.]+ ms' \
            | sed -E 's/time=([0-9.]+) ms/\1/g' \
            | sort -n \
            | sed -n "$((($PING_COUNT + 1) / 2))p" \
            > ${TMP_DIR}/results/median-rtt-c-w1.txt
        else
          echo "Not running ping time experiment."
        fi
      fi
      # Clean up AWS resources
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
