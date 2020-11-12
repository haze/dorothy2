# Build Stage
FROM clux/muslrust:1.49.0-nightly-2020-10-04 AS builder
WORKDIR /usr/src/
RUN rustup target add x86_64-unknown-linux-musl

RUN USER=root cargo new dorothy
WORKDIR /usr/src/dorothy
COPY Cargo.toml Cargo.lock ./
ENV LIBOPUS_STATIC=1
ENV PKG_CONFIG_ALLOW_CROSS=1 
RUN cargo build --release

COPY src ./src
RUN cargo install --target x86_64-unknown-linux-musl --path .
RUN apt-get update && apt-get -y install ca-certificates && rm -rf /var/lib/apt/lists/*

# Bundle Stage
FROM scratch

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /usr/src/dorothy/target/x86_64-unknown-linux-musl/release/dorothy .
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_DIR=/etc/ssl/certs

CMD ["./dorothy"]
