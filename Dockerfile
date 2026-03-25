FROM rust:latest as builder

WORKDIR /app

# cache dependency build
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

# build the application
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

WORKDIR /app

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
CMD ["./paste"]
