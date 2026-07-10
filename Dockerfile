FROM rust:1.96-bookworm AS builder
ARG FEATURES=portable-tls
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential ca-certificates clang cmake ninja-build perl pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release --no-default-features --features "${FEATURES}"

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 extractor
COPY --from=builder /src/target/release/rama-controlled-extractor /usr/local/bin/rama-controlled-extractor
WORKDIR /data
USER extractor
EXPOSE 8090
ENTRYPOINT ["rama-controlled-extractor"]
CMD ["serve", "--config", "/data/config/example.yaml", "--bind", "0.0.0.0:8090", "--allow-remote", "--no-open"]
