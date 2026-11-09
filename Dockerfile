# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin kawalo-api --bin rotate_key

FROM debian:bookworm-slim
# tzdata is required: `operational` results depend on the process time zone (TZ).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /app kawalo
WORKDIR /app
COPY --from=build /app/target/release/kawalo-api /app/target/release/rotate_key /usr/local/bin/
# certs/ (RSA keys) is mounted at runtime, never baked into the image:
#   docker run -v "$PWD/certs:/app/certs" --env-file .env.prod ...
ENV TZ=Asia/Jakarta NODE_ENV=production PORT=3000
USER kawalo
EXPOSE 3000
CMD ["kawalo-api"]
