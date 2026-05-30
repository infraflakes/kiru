FROM rust:alpine AS builder
WORKDIR /src
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch AS bin
COPY --from=builder /src/target/x86_64-unknown-linux-musl/release/kiru /
