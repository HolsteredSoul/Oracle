# Build stage
FROM rust:1.77-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/oracle /usr/local/bin/
COPY config.toml /etc/oracle/
CMD ["oracle", "--config", "/etc/oracle/config.toml"]
