FROM rust:slim-bookworm

WORKDIR /workspace

ENV PATH=/root/.local/bin:$PATH

RUN apt update && \
    apt install -y curl git && \
    curl -LsSf https://astral.sh/uv/install.sh | sh && \
    echo 'eval "$(uv generate-shell-completion bash)"' >> ~/.bashrc && \
    cargo install prek
