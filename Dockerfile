FROM rust as build
RUN mkdir -p /app
COPY runtime  /app/runtime
COPY node /app/node
COPY pallets /app/pallets
COPY shell.nix .envrc rustfmt.toml Cargo.toml Cargo.lock /app/
WORKDIR /app
RUN apt-get update
RUN apt install -y git cmake clang curl libssl-dev llvm libudev-dev
RUN rustup default stable
RUN rustup update nightly
RUN rustup update stable
RUN rustup target add wasm32-unknown-unknown --toolchain nightly
RUN cargo build --release

FROM debian:buster-slim as runtime
RUN apt update
RUN apt install -y git cmake clang curl libssl-dev llvm libudev-dev
RUN mkdir /app
RUN mkdir /app/target
COPY --from=build /app/target /app/target
WORKDIR /app/target
EXPOSE 9944 30333 9933

ENTRYPOINT ["/app/target/"]