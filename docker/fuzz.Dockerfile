# embnet-fuzz：cargo-fuzz 需要 nightly（docs/40-rust-embedded/03 第二節）。nightly 釘在固定日期。
FROM rust:1.100.0-slim-bookworm@sha256:7c7d4eb4d05b7781187a327809466cfddf29753f6defdc3d19a0e291951b7a37

RUN apt-get update \
 && apt-get install -y --no-install-recommends g++ ca-certificates git \
 && rm -rf /var/lib/apt/lists/*

RUN rustup toolchain install nightly-2026-11-01 --profile minimal --component llvm-tools \
 && rustup default nightly-2026-11-01 \
 && cargo install --locked cargo-fuzz@0.13.2 \
 && rm -rf /usr/local/cargo/registry /usr/local/cargo/git
