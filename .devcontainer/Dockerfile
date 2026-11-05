FROM mcr.microsoft.com/devcontainers/base:trixie

# clang/libclang: only needed to regenerate PCAN bindings (scripts/gen-pcan-bindings.sh)
# can-utils: candump/cansend etc. for testing against SocketCAN interfaces
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive && \
    apt-get -y install --no-install-recommends \
        clang libclang-dev can-utils ripgrep tmux curl ca-certificates && \
    apt-get autoremove -y && apt-get clean -y && rm -rf /var/lib/apt/lists/*

# uv (Python package manager)
COPY --from=ghcr.io/astral-sh/uv:0.11.15 /uv /uvx /bin/

# Rust: the toolchain version is pinned in rust-toolchain.toml
USER vscode
ENV RUSTUP_HOME=/home/vscode/.rustup \
    CARGO_HOME=/home/vscode/.cargo \
    PATH=/home/vscode/.cargo/bin:$PATH
COPY --chown=vscode:vscode rust-toolchain.toml /tmp/rust-toolchain.toml
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
        sh -s -- -y --no-modify-path --profile minimal --default-toolchain none && \
    cd /tmp && rustup toolchain install && \
    rustup default "$(rustup show active-toolchain | cut -d' ' -f1)" && \
    rm /tmp/rust-toolchain.toml
