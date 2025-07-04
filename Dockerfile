# syntax=docker/dockerfile:1

FROM rust:1.88 AS builder
WORKDIR /usr/src/escli
# Install musl-tools and cross-compiler for arm64
RUN apt-get update && apt-get install -y musl-tools gcc-aarch64-linux-gnu && rm -rf /var/lib/apt/lists/*
COPY . .
# Install Rust targets for musl cross-compilation (amd64 and arm64)
RUN rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
# Run code generation before building escli
RUN cargo run -p generator --release
# Set build args for target and linker
ARG CARGO_BUILD_TARGET=x86_64-unknown-linux-musl
ENV CARGO_BUILD_TARGET=${CARGO_BUILD_TARGET}
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc
RUN cargo build -p escli --release --target=${CARGO_BUILD_TARGET}

FROM scratch
ARG CARGO_BUILD_TARGET=x86_64-unknown-linux-musl
COPY --from=builder /usr/src/escli/target/${CARGO_BUILD_TARGET}/release/escli /escli
ENTRYPOINT ["/escli"]