# Use the official Rust image
FROM rustlang/rust:nightly  as builder

# Create app directory
WORKDIR /usr/src/app

# # Pre-cache dependencies
# COPY Cargo.toml Cargo.lock ./
# RUN mkdir src && echo "fn main() {}" > src/main.rs
# RUN cargo build --release && rm -r src

# Copy actual source
COPY . .

# Build actual binary
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

# Install SSL certs (important for HTTPS and reqwest clients)
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
# Copy the built binary from the builder stage
COPY --from=builder /usr/src/app/target/release/builder_user /usr/local/bin/builder_user

COPY --from=builder /usr/src/app/builder_user/config.toml /etc/build_server.toml

# Expose the port your app uses
EXPOSE 8080

# Run the app
CMD ["builder_user"]
