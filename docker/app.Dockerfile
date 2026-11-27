# The desktop app's Linux packages, built where they run: on an
# older, ordinary distribution (Ubuntu 22.04, glibc 2.35), so the AppImage, .deb
# and .rpm work on most of them. `make app-portable` runs it; the
# packages land in target/app-dist/.

FROM node:26.13.0-trixie-slim AS web
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/ui/package.json apps/ui/
COPY sdk sdk
RUN npm install --global "$(node --print 'require("./package.json").packageManager')" \
    && pnpm install --frozen-lockfile --ignore-scripts --filter @ahab/ui
COPY apps/ui apps/ui
COPY crates/ahab/openapi.json crates/ahab/
RUN pnpm --filter @ahab/ui build

FROM ubuntu:26.04 AS build
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        build-essential ca-certificates curl file git pkg-config xdg-utils \
        libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev \
        libayatana-appindicator3-dev libdbus-1-dev \
    && rm -rf /var/lib/apt/lists/*
ENV PATH=/root/.cargo/bin:$PATH
COPY rust-toolchain.toml /tmp/
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal \
            --default-toolchain "$(sed -n 's/^channel = "\(.*\)"/\1/p' /tmp/rust-toolchain.toml)" \
    && cargo install tauri-cli --version 2.12.4 --locked
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
# Embedded in the binary: the tool registry and the languages.
# The web app (apps/ui/dist) is also the app's own screens (one UI).
COPY tools tools
COPY apps/ui/languages apps/ui/languages
COPY --from=web /src/apps/ui/dist apps/ui/dist
# Cargo's downloads and build folders persist between builds (BuildKit cache
# mounts), so a rebuild compiles only what changed. They are not part of the
# image: each step copies its result out to /out.
RUN --mount=type=cache,id=ahab-app-cargo-registry,target=/root/.cargo/registry \
    --mount=type=cache,id=ahab-app-cargo-git,target=/root/.cargo/git \
    --mount=type=cache,id=ahab-app-target,target=/src/target \
    cargo build --release --locked --bin ahab \
    && mkdir -p /out \
    && cp target/release/ahab /out/ahab
COPY apps/native apps/native
# Libraries linuxdeploy leaves out as "on every system" that minimal
# hosts (NixOS's appimage-run, slim distributions) may lack: bundled in the
# AppImage too (the .deb and .rpm depend on the distribution's).
ARG APPIMAGE_EXTRA_LIBS="libgpg-error.so.0"
COPY docker/app-bundle.sh /usr/local/bin/app-bundle
RUN --mount=type=cache,id=ahab-app-cargo-registry,target=/root/.cargo/registry \
    --mount=type=cache,id=ahab-app-cargo-git,target=/root/.cargo/git \
    --mount=type=cache,id=ahab-app-native-target,target=/src/apps/native/target \
    mkdir -p apps/native/binaries \
    && cp /out/ahab "apps/native/binaries/ahab-$(rustc -vV | sed -n 's/^host: //p')" \
    && cd apps/native \
    && rm -rf target/release/bundle \
    && APPIMAGE_EXTRACT_AND_RUN=1 app-bundle $APPIMAGE_EXTRA_LIBS \
    && mkdir -p /out/bundle \
    && cp target/release/bundle/appimage/*.AppImage target/release/bundle/deb/*.deb \
        target/release/bundle/rpm/*.rpm /out/bundle/

FROM scratch
COPY --from=build /out/bundle/ /
