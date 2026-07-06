# Stage 1: Build Next.js static export
FROM node:24-alpine AS frontend
RUN corepack enable
WORKDIR /app
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY src/ ./src/
COPY public/ ./public/
COPY next.config.ts tsconfig.json postcss.config.mjs components.json ./
RUN pnpm build

# Stage 2: Build Rust server binary + iyw-claw-mcp companion
FROM rust:slim-bookworm AS backend
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app/src-tauri
COPY src-tauri/ ./
# iyw-claw-mcp is the stdio MCP companion the runtime injects per session
# (see acp/delegation/companion.rs). It must ship next to iyw-claw-server so
# `locate_iyw_claw_mcp_binary()` finds it via the exe-sibling lookup.
RUN cargo build --release --bin iyw-claw-server --no-default-features \
 && cargo build --release --bin iyw-claw-mcp --no-default-features

# Stage 3: Runtime
FROM node:24-bookworm-slim
RUN apt-get update && apt-get install -y \
    libsqlite3-0 \
    git \
    openssh-client \
    ca-certificates \
    curl \
    python3 \
    python3-pip \
    libicu72 \
    && rm -rf /var/lib/apt/lists/*
# libicu72: OfficeCLI ships as a self-contained binary with an embedded .NET
# runtime, which requires the system ICU library at startup. node:*-bookworm-slim
# bundles Node's own ICU statically and so does NOT install system libicu — without
# this, every `officecli` invocation aborts with "Couldn't find a valid ICU package
# installed on the system", breaking both skill sync and office file preview in the
# server/Docker mode. The version (72) is pinned to Debian bookworm; bump it to match
# if the base image moves to a newer Debian release (e.g. trixie ships libicu76).

COPY --from=backend /app/src-tauri/target/release/iyw-claw-server /usr/local/bin/iyw-claw-server
COPY --from=backend /app/src-tauri/target/release/iyw-claw-mcp /usr/local/bin/iyw-claw-mcp
COPY --from=frontend /app/out /app/web

ENV IYW_CLAW_STATIC_DIR=/app/web
ENV IYW_CLAW_DATA_DIR=/data
ENV IYW_CLAW_PORT=3080
ENV IYW_CLAW_HOST=0.0.0.0
ENV SHELL=/bin/bash
# In-place self-update markers: tells the running server it is a container
# (for the post-upgrade "also pull the image" hint) and how long the
# supervisor waits before relaunching the worker after an upgrade.
ENV IYW_CLAW_RUNTIME=docker
ENV IYW_CLAW_RESTART_DELAY_MS=2000

EXPOSE 3080
VOLUME /data

# Run under the built-in supervisor (PID 1) so an in-place upgrade can swap
# the binary and have the worker relaunched without stopping the container.
CMD ["iyw-claw-server", "--supervise"]
