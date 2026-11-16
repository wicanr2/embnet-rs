# embnet-probe：probe-rs 燒錄與讀記憶體（docs/plans/004 第二節；P5 起用到實板）。
FROM rust:1.100.0-slim-bookworm@sha256:7c7d4eb4d05b7781187a327809466cfddf29753f6defdc3d19a0e291951b7a37

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libudev-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

RUN cargo install --locked probe-rs-tools@0.32.0 \
 && rm -rf /usr/local/cargo/registry /usr/local/cargo/git
