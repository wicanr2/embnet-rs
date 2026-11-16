#!/bin/sh
# 在 embnet-rust 容器裡執行：編 baseline 與 libcanard 兩個 ELF，輸出 size -A。
set -eu
CFLAGS="-Os -mcpu=cortex-m7 -mthumb -mfloat-abi=hard -mfpu=fpv5-d16 -std=c11 -ffunction-sections -fdata-sections -fno-common -DNDEBUG"
LDFLAGS="-nostartfiles -Wl,--gc-sections -T linker.ld --specs=nano.specs --specs=nosys.specs"
mkdir -p build
arm-none-eabi-gcc $CFLAGS $LDFLAGS -o build/size-baseline-c.elf shell.c baseline.c
arm-none-eabi-gcc $CFLAGS -Ithird_party/libcanard/libcanard -Ithird_party/libcanard/lib/cavl2 $LDFLAGS \
  -o build/size-libcanard.elf shell.c libcanard_probe.c third_party/libcanard/libcanard/canard.c
for e in size-baseline-c size-libcanard; do arm-none-eabi-size -A build/$e.elf > build/$e.size; done
cat build/size-baseline-c.size build/size-libcanard.size
