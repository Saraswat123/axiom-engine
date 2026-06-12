# Multi-stage build — minimal final image
FROM rust:1.93-bookworm AS builder

WORKDIR /app

# Install z3 system library
RUN apt-get update && apt-get install -y libz3-dev clang && rm -rf /var/lib/apt/lists/*

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY guest/ guest/
COPY .cargo/ .cargo/

RUN cargo build --release -p axiom-mcp-server

# Runtime image — minimal
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y libz3-4 ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/axiom-mcp /usr/local/bin/axiom-mcp

ENV AXIOM_TRANSPORT=http
ENV PORT=8080
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s \
    CMD curl -f http://localhost:8080/health || exit 1

CMD ["axiom-mcp"]
