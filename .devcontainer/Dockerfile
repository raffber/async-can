FROM mcr.microsoft.com/devcontainers/base:trixie

USER vscode
RUN sudo apt-get update && export DEBIAN_FRONTEND=noninteractive && \
    sudo apt-get -y install --no-install-recommends \
    python-is-python3 python3-venv curl gnupg zip unzip liblzma-dev ripgrep xz-utils tmux clang && \
    sudo apt-get autoremove -y && sudo apt-get clean -y && sudo rm -rf /var/lib/apt/lists/*

# uv (Python package manager)
COPY --from=ghcr.io/astral-sh/uv:0.11.15 /uv /uvx /bin/

# Rust
USER vscode
ENV RUSTUP_HOME=/home/vscode/.rustup \
    CARGO_HOME=/home/vscode/.cargo \
    PATH=/home/vscode/.cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain=1.94.0 && \
    rustup component add rustfmt clippy && \
    rustup target add x86_64-unknown-linux-gnu
