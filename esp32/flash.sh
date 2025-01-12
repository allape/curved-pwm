#!/usr/bin/bash

flash_size="2mb"
if [ "$1" == "4mb" ]; then
  flash_size="4mb"
fi

cargo build --release
cargo espflash flash \
  --release \
  --baud 921600 \
  --chip esp32c3 \
  --flash-size "$flash_size" \
  --partition-table "partitions_singleapp_large_$flash_size.csv" \
  --monitor
