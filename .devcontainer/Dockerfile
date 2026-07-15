FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends nodejs npm ripgrep \
    && npm install -g lore-protocol \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-codspeed --locked
RUN rustup component add llvm-tools-preview \
    && cargo install cargo-llvm-cov --locked