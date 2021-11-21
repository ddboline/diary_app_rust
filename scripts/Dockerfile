FROM rust:alpine3.14 as buildstage

RUN apk add musl-dev && mkdir -p /build/diary_app_rust

ADD Cargo.toml /build/diary_app_rust/
COPY src /build/diary_app_rust/src
COPY diary_app_api /build/diary_app_rust/diary_app_api
COPY diary_app_bot /build/diary_app_rust/diary_app_bot
COPY diary_app_lib /build/diary_app_rust/diary_app_lib
COPY migrations /build/diary_app_rust/migrations
COPY templates /build/diary_app_rust/templates

RUN mkdir -p /diary_app_rust && \
    cd /build/diary_app_rust && \
    cargo install --path=. --root=/diary_app_rust

FROM alpine:3.14

COPY --from=buildstage /diary_app_rust/bin/* /usr/bin/

RUN apk add openssh-client

STOPSIGNAL SIGTERM

EXPOSE 3042

CMD ["/usr/bin/aws-app-http"]
