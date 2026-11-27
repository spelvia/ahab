# The Ahab image: the one `ahab` binary (server and command line) and the
# built UI.
# Build from the repository root: docker build -f docker/Dockerfile .

FROM node:26.13.0-trixie-slim AS ui
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/ui/package.json apps/ui/
RUN npm install --global "$(node --print 'require("./package.json").packageManager')" \
    && pnpm install --frozen-lockfile --ignore-scripts --filter @ahab/ui
COPY apps/ui apps/ui
COPY crates/ahab/openapi.json crates/ahab/
RUN pnpm --filter @ahab/ui build && chmod -R a+rX apps/ui/dist

FROM rust:1.99.1-slim-trixie AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
# The release binary carries the built web app (crates/ahab/src/server/web.rs).
COPY --from=ui /src/apps/ui/dist apps/ui/dist
RUN cargo build --release --locked --bin ahab

FROM debian:trixie-20261005-slim
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --create-home --home-dir /home/ahab ahab \
    && mkdir /data && chown ahab:ahab /data
COPY --from=server /src/target/release/ahab /usr/local/bin/
# config.toml and the SQLite database in /data; set DATABASE_URL to a
# postgres:// URL instead. A daemon in a container could only see the
# container, so it is off.
ENV AHAB_CONFIG_DIR=/data \
    AHAB_DATA_DIR=/data \
    AHAB_DAEMON=false \
    AHAB_LISTEN_ADDR=0.0.0.0:8080 \
    AHAB_LOG_FORMAT=json
USER ahab
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["tini", "--", "ahab"]
CMD ["up", "--foreground"]
