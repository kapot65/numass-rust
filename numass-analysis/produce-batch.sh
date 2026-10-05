#!/bin/bash

RUN="2025_05"
FILL=Tritium_4

exceptions=("26_bad")

for file in /data-fast/numass-server/$RUN/$FILL/*; do   
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
run: $RUN
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
    merge_frames: 5000
    merge_splits_first: false
    merge_close_events: true
    ignore_borders: false
    ignore_channels:
     ch1: false
     ch2: false
     ch3: false
     ch4: false
     ch5: false
     ch6: false
     ch7: false
groups:
    $FILL:
        $set_n:
monitor: "/data-fast/monitor_05_2025.json"
EOM

cargo run --release --bin produce-data-cli -- $config_name

done




