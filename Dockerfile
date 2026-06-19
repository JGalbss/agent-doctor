# Reproducible agent-doctor image — no local Rust toolchain needed.
# Build:  docker build -t agent-doctor .
# Use:    docker run --rm -v "$PWD:/repo" agent-doctor gate --base main
# MCP:    docker run --rm -i -v "$PWD:/repo" agent-doctor serve --mcp

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p agent-doctor

FROM debian:bookworm-slim
# git is required for the diff/gate/impact/merge subcommands.
RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/agent-doctor /usr/local/bin/agent-doctor
# Operate on a repo mounted at /repo.
WORKDIR /repo
ENTRYPOINT ["agent-doctor"]
CMD ["--help"]
