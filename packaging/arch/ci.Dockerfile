# Build from a git archive (no working tree, credentials, or host devices).
ARG NODE_VERSION=24.15.0
FROM node:${NODE_VERSION}-bookworm-slim AS node
FROM archlinux:base-devel AS build
RUN pacman -Syu --noconfirm --needed git python rustup webkit2gtk-4.1 gtk3 \
    desktop-file-utils fakeroot libpulse && useradd --create-home builder
COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules/npm /opt/npm
RUN ln -s /opt/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s /opt/npm/bin/npx-cli.js /usr/local/bin/npx
USER builder
WORKDIR /home/builder/source
COPY --chown=builder:builder . .
ENV PATH=/home/builder/.local/bin:/home/builder/.cargo/bin:/usr/local/bin:/usr/bin
RUN rustup show active-toolchain && test "$(node -p process.versions.node)" = "$(cat .node-version)" \
    && cd apps/desktop && /usr/local/bin/npm install --global --prefix /home/builder/.local "$(node -p 'require("./package.json").packageManager')"
# Fresh shell: resolve npm from the new prefix, not the bootstrap shell's cache.
RUN cd apps/desktop && test "$(command -v npm)" = /home/builder/.local/bin/npm \
    && test "npm@$(npm --version)" = "$(node -p 'require("./package.json").packageManager')" && npm ci
ARG SOURCE_DATE_EPOCH
ARG PKGREL
RUN set -eu; export SOURCE_DATE_EPOCH; \
    PKGREL=$((PKGREL-1)) bash packaging/arch/build.sh; \
    bash packaging/arch/build.sh; \
    version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml","rb"))["workspace"]["package"]["version"])'); \
    prefix="target/packages/fw16-ledmatrix-$version"; \
    new="$prefix-$PKGREL-x86_64.pkg.tar.zst"; \
    container=docker bash packaging/arch/check-package.sh --isolated-container "$prefix-$((PKGREL-1))-x86_64.pkg.tar.zst" "$new"; \
    mkdir ../tested; cp "$new" ../tested/; cd ../tested; sha256sum *.pkg.tar.zst > SHA256SUMS
FROM scratch AS artifact
COPY --from=build /home/builder/tested/ /
