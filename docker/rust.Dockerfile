# embnet-rust：建置、體積量測與主機測試用的鎖版環境。版本選擇見 docs/plans/004 第二節。
FROM rust:1.100.0-slim-bookworm@sha256:7c7d4eb4d05b7781187a327809466cfddf29753f6defdc3d19a0e291951b7a37

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      gcc-arm-none-eabi libnewlib-arm-none-eabi \
      iproute2 can-utils rt-tests ca-certificates git \
 && rm -rf /var/lib/apt/lists/*

RUN rustup target add thumbv7em-none-eabihf \
 && rustup component add llvm-tools clippy rustfmt

RUN cargo install --locked cargo-binutils@0.4.0 flip-link@0.1.12 \
 && rm -rf /usr/local/cargo/registry /usr/local/cargo/git
