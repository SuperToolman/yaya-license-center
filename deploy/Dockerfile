FROM rust:1-bookworm AS api-builder
WORKDIR /workspace/api
ARG CARGO_REGISTRIES_CRATES_IO_INDEX=sparse+https://rsproxy.cn/index/
RUN mkdir -p /usr/local/cargo \
    && printf '[source.crates-io]\nreplace-with = "license-center-mirror"\n\n[source.license-center-mirror]\nregistry = "%s"\n' "$CARGO_REGISTRIES_CRATES_IO_INDEX" > /usr/local/cargo/config.toml
ENV CARGO_HTTP_TIMEOUT=45 \
    CARGO_NET_RETRY=2 \
    CARGO_TERM_PROGRESS_WHEN=never
COPY api/Cargo.toml api/Cargo.lock ./
COPY api/src ./src
RUN cargo build --release --verbose

FROM node:22-bookworm-slim AS web-builder
WORKDIR /workspace
RUN corepack enable
COPY pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web/package.json web/package.json
RUN pnpm install --frozen-lockfile
COPY web ./web
ENV NEXT_PUBLIC_LICENSE_API_BASE_URL=/license-api
RUN pnpm --dir web build

FROM node:22-bookworm-slim
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=api-builder /workspace/api/target/release/yaya-license-center-api /app/api/yaya-license-center-api
COPY --from=web-builder /workspace/web/.next/standalone ./
COPY --from=web-builder /workspace/web/.next/static ./web/.next/static
COPY deploy/entrypoint.sh /app/entrypoint.sh
RUN chmod 755 /app/entrypoint.sh /app/api/yaya-license-center-api
ENV NODE_ENV=production \
    LICENSE_CENTER_HOST=0.0.0.0 \
    LICENSE_CENTER_PORT=8779 \
    LICENSE_CENTER_DB_PATH=/var/lib/yaya-license-center/license-center.sqlite3 \
    PORT=8778 \
    HOSTNAME=0.0.0.0
VOLUME ["/var/lib/yaya-license-center"]
EXPOSE 8778
ENTRYPOINT ["/app/entrypoint.sh"]
