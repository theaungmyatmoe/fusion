default: run

build:
    cargo build --release

run *ARGS:
    cargo run -p fusion-cli -- {{ARGS}}

dev *ARGS:
    cargo run -p fusion-cli -- {{ARGS}}

test:
    cargo test --workspace

check:
    cargo clippy --workspace -- -D warnings

fmt:
    cargo fmt --all

clean:
    cargo clean
