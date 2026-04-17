FROM rust:latest AS builder

WORKDIR /app

COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

WORKDIR /app

LABEL org.opencontainers.image.source="https://github.com/s1dny/paste"

# install necessary runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# copy the binary and necessary files from the builder
COPY --from=builder /app/target/release/paste /app/paste
COPY --from=builder /app/static /app/static

# expose the port the app runs on
EXPOSE 3000

# run the binary
ENTRYPOINT ["/app/paste"]
