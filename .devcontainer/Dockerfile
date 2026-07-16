FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends nodejs npm ripgrep \
    && npm install -g lore-protocol \
    && rm -rf /var/lib/apt/lists/*

# Install Rust CLI tools as the runtime user to avoid root-owned Cargo registry artifacts.
USER vscode
RUN rustup component add llvm-tools-preview \
    && cargo install cargo-codspeed --locked \
    && cargo install cargo-llvm-cov --locked