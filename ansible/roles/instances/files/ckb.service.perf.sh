#!/bin/bash
if [[ $1 == perf ]]; then
    echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid
    sudo sed -i 's_ExecStart=/usr/local/bin/ckb run_ExecStart=/home/ubuntu/.cargo/bin/flamegraph -o /tmp/donotgen/flamegraph.svg /usr/local/bin/ckb run_g' /etc/systemd/system/ckb.service
    sudo systemctl daemon-reload
    sudo systemctl restart ckb.service
else
    echo 3 | sudo tee /proc/sys/kernel/perf_event_paranoid
    sudo sed -i 's_ExecStart=/home/ubuntu/.cargo/bin/flamegraph -o /tmp/donotgen/flamegraph.svg /usr/local/bin/ckb run_ExecStart=/usr/local/bin/ckb run_g' /etc/systemd/system/ckb.service
    sudo systemctl daemon-reload
    sudo systemctl restart ckb.service
fi
