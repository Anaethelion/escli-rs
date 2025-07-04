# syntax=docker/dockerfile:1

FROM rust:1.88 AS builder
WORKDIR /usr/src/escli
# Install musl-tools for cross-compilation
RUN apt-get update && apt-get install -y musl-tools && rm -rf /var/lib/apt/lists/*
COPY . .
# Install Rust target for musl cross-compilation (amd64 only)
RUN rustup target add x86_64-unknown-linux-musl
# Run code generation before building escli
RUN cargo run -p generator --release
# Build for amd64 musl target from workspace root
RUN cargo build -p escli --release --target=x86_64-unknown-linux-musl


FROM scratch
COPY --from=builder /usr/src/escli/target/x86_64-unknown-linux-musl/release/escli /escli
ENTRYPOINT ["/escli"]
CMD ["--help"]