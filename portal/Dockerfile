FROM rust:1.90-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY portal/Cargo.toml portal/Cargo.toml
COPY portal/src portal/src
COPY migrations migrations
RUN cargo build --locked --release -p vpn-rdp-portal

FROM debian:bookworm-slim
RUN useradd --create-home --uid 10001 portal
WORKDIR /app
COPY --from=build /src/target/release/vpn-rdp-portal /usr/local/bin/vpn-rdp-portal
COPY portal/web web
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/vpn-rdp-portal"]
