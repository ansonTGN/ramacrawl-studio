.PHONY: serve validate crawl dashboard verify fmt check test clippy check-boring docker-build

CONFIG ?= config/example.yaml

serve:
	cargo run --release -- serve --config $(CONFIG)

validate:
	cargo run --release -- validate --config $(CONFIG)

crawl:
	cargo run --release -- crawl --config $(CONFIG)

dashboard:
	cargo run --release -- dashboard --config $(CONFIG) --output output/dashboard.html

verify:
	bash scripts/verify.sh

fmt:
	cargo fmt --all

check:
	cargo check --all-targets

test:
	cargo test --all-targets

clippy:
	cargo clippy --all-targets -- -D warnings

check-boring:
	cargo check --all-targets --no-default-features --features boring-emulation

docker-build:
	docker compose build
