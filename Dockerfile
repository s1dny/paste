FROM rust:latest as builder

WORKDIR /app

# copy the entire project
COPY . .

# build the application
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
COPY --from=builder /app/wordlist.txt /app/wordlist.txt

# expose the port the app runs on
EXPOSE 3000

# run the binary
CMD ["./paste"]
