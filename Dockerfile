# Multi-stage build — minimal final image
FROM rust:1.93-bookworm AS builder

WORKDIR /app

# Install z3 + clang (bindgen needs clang)
RUN apt-get update && apt-get install -y libz3-dev clang && rm -rf /var/lib/apt/lists/*

# Override .cargo/config.toml macOS path — Linux z3 header is at /usr/include/z3.h
ENV Z3_SYS_Z3_HEADER=/usr/include/z3.h

# Copy source
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY guest/ guest/
COPY .cargo/ .cargo/

# Build release binary
RUN cargo build --release -p axiom-mcp-server

# Runtime image — minimal Debian
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y libz3-4 ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/axiom-mcp /usr/local/bin/axiom-mcp

ENV AXIOM_TRANSPORT=http
ENV PORT=8080
EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -sf http://localhost:8080/health || exit 1

CMD ["axiom-mcp"]
