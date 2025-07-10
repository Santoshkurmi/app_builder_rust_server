# --- Stage 1: Rust Builder ---
FROM rustlang/rust:nightly as builder

WORKDIR /usr/src/app

COPY Cargo.lock ./
COPY Cargo_Cache.toml ./Cargo.toml
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -r src

COPY . .
RUN cargo build --release

# --- Stage 2: Full Runtime + Android Build Environment ---
FROM debian:bookworm-slim

RUN mkdir -p /home/cat/school_app


# Install system packages
RUN apt-get update && apt-get install -y \
    curl wget imagemagick locales unzip zip git neovim zsh sudo \
 && rm -rf /var/lib/apt/lists/*




RUN sed -i 's/# en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/' /etc/locale.gen \
 && locale-gen

ENV LANG=en_US.UTF-8 \
    LANGUAGE=en_US:en \
    LC_ALL=en_US.UTF-8




WORKDIR /home/cat/project

COPY --from=builder /usr/src/app/target/release/builder_user /usr/bin/builder_user
COPY --from=builder /usr/src/app/config.toml /etc/config.toml
COPY --from=builder /usr/src/app/brightschool_app/ /home/cat/school_app/



# Expose the app port
EXPOSE 8080

# Start the Rust binary
CMD ["builder_user"]
