SHELL := /bin/bash
.ONESHELL:
.SHELLFLAGS := -eufo pipefail -c
MAKEFLAGS += --always-make
.SILENT:

curdir := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))
projdir := $(shell git rev-parse --show-toplevel)

help:
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Development

format: ## Format the code
	cd $(projdir)
	cargo fmt --all

check: ## Run cspell, cargo check and cargo clippy
	cd $(projdir)
	npx --yes cspell@9 --no-progress --gitignore "**"
	cargo check --all-features --all-targets
	cargo clippy --all-features --all-targets

build: ## Build the crate
	cd $(projdir)
	cargo build --all-features --all-targets

test: ## Run the tests
	cd $(projdir)
	cargo test --all-features

bindgen: ## Regenerate the PCAN bindings from include/PCANBasic.h
	$(projdir)/scripts/gen-pcan-bindings.sh
