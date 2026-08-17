UV ?= uv

.PHONY: uv-sync py-lint py-format py-test-vault-sync py-test rust-test rust-check shell-check agent-install-test docs-check test check release-check fmt

uv-sync:
	$(UV) sync --group dev

py-lint:
	$(UV) run ruff check tools

py-format:
	$(UV) run ruff format tools

py-test-vault-sync:
	$(UV) run python -m unittest discover -s tools/vault_sync/tests -v

py-test: py-test-vault-sync

rust-test:
	cargo test

rust-check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

shell-check:
	bash -n scripts/*.sh

agent-install-test:
	scripts/test-install.sh

docs-check:
	python3 tools/check_docs.py

test: rust-test py-test

check: rust-check py-lint py-test shell-check agent-install-test docs-check

release-check: check
	cargo build --release --locked -p memento-cli -p mementod -p memento-mcp

fmt:
	cargo fmt --all
	$(UV) run ruff format tools
