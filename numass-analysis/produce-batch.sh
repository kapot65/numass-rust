#!/bin/bash

FILL=Tritium_4

exceptions=("26_bad")

for file in /data-fast/numass-server/2024_11/$FILL/*; do   
        set=$(basename $file)
        set_n=${set:4}
        echo "Processing: ${set:4}"

        # Check if set_n is in the exceptions list
        if [[ " ${exceptions[@]} " =~ " ${set_n} " ]]; then
            echo "Skipping set_n: ${set_n}"
            continue
        fi

        config_name="./${FILL}_${set}.yaml"

cat > $config_name <<-EOM
db_root: "/data-fast/numass-server"
run: 2024_11
processing:
    algorithm: !Trapezoid
        left: 6
        center: 15
        right: 6
        treshold: 16
        min_length: 16
        skip: Bad
        reset_detection:
            window: 10
            treshold: 800
            size: 110
    convert_to_kev: true
post_processing:
    cut_bad_blocks: true
    merge_splits_first: false
    merge_close_events: true
    ignore_borders: false
    ignore_channels:
    - false
    - false
    - false
    - false
    - false
    - false
    - false
groups:
    $FILL:
        $set_n:
monitor: "/data-fast/monitor_11_2024.json"
EOM

cargo run --release --bin produce-data-cli -- $config_name

done




