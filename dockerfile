FROM rust:1.62

WORKDIR /collar
COPY ./ .

RUN cargo install --path .

CMD ["./colla-rs-sever"]