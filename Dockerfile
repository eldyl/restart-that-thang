FROM lukemathwalker/cargo-chef:latest-rust-1.87-alpine AS chef

WORKDIR /app
RUN apk update && apk add --no-cache lld clang

FROM chef AS planner
COPY . .
RUN cargo +nightly chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

RUN cargo +nightly build --release --bin restart-that-thang

FROM alpine:latest AS runtime
WORKDIR /app
RUN apk update && apk add --no-cache ca-certificates docker-cli

COPY --from=builder /app/target/release/restart-that-thang restart-that-thang

ENTRYPOINT ["./restart-that-thang"]
