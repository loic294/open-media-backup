FROM rust:1-alpine AS build
RUN apk add --no-cache build-base
WORKDIR /build
ARG OMB_HASH_VERSION
ENV OMB_HASH_VERSION=${OMB_HASH_VERSION}
COPY src-tauri/ src-tauri/
RUN cargo build --locked --release --manifest-path src-tauri/Cargo.toml -p omb-hash-server \
    && mkdir /config && chown 65532:65532 /config

FROM scratch
COPY --from=build /build/src-tauri/target/release/omb-hash-server /omb-hash-server
COPY --from=build --chown=65532:65532 /config /config
USER 65532:65532
EXPOSE 47822
VOLUME ["/config"]
ENTRYPOINT ["/omb-hash-server"]
