SHELL := /bin/sh

BIN_NAME := gitsnitch
CARGO := $(shell command -v cargo 2>/dev/null || echo $(HOME)/.cargo/bin/cargo)
JSONSCHEMA := $(shell [ -x "$(HOME)/.cargo/bin/jsonschema-cli" ] && echo "$(HOME)/.cargo/bin/jsonschema-cli" || command -v jsonschema-cli 2>/dev/null || command -v jsonschema 2>/dev/null || echo jsonschema-cli)
PLANTUML := $(shell command -v plantuml 2>/dev/null || echo plantuml)
SCHEMA_DOC := $(shell command -v generate-schema-doc 2>/dev/null || echo generate-schema-doc)
JQ := $(shell command -v jq 2>/dev/null || echo jq)

ifeq ($(OS),Windows_NT)
EXE_EXT := .exe
else
EXE_EXT :=
endif

BIN_DIR := bin
BIN_PATH := $(BIN_DIR)/$(BIN_NAME)$(EXE_EXT)

.PHONY: help local build test clippy fmt check quality clean install-tools validate-examples update-api-design-svg update-api-schema-md fmt-json docs maintenance

help:
	@echo "Targets:"
	@echo "  local   - fmt + clippy + test + build (host-native)"
	@echo "  build   - build release binary for current host"
	@echo "  test    - run tests"
	@echo "  clippy  - run clippy with warnings denied"
	@echo "  fmt     - check formatting"
	@echo "  check   - cargo check"
	@echo "  quality - run core quality gates (fmt + clippy + test + check)"
	@echo "  maintenance - update lockfile + dependency hygiene checks"
	@echo "  docs    - generate Rust API docs (no deps)"
	@echo "  clean   - remove build artifacts"
	@echo "  install-tools - install contributor CLI tools"
	@echo "  validate-examples - validate API example JSON against schema"
	@echo "  update-api-design-svg - regenerate docs/api_design/api_design.svg from PlantUML"
	@echo "  update-api-schema-md - generate Markdown documentation from JSON schema"
	@echo "  fmt-json - prettify JSON files (schema and example)"

local: fmt clippy test build

build:
	@echo "Building $(BIN_NAME) for current host"
	$(CARGO) build --release
	mkdir -p $(BIN_DIR)
	cp target/release/$(BIN_NAME)$(EXE_EXT) $(BIN_PATH)
	@echo "Built: $(BIN_PATH)"

test:
	$(CARGO) test --all-features

clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

fmt:
	$(CARGO) fmt --check

check:
	$(CARGO) check --all-targets --all-features

quality: fmt clippy test check

maintenance:
	$(CARGO) update
	$(CARGO) machete
	$(CARGO) deny check
	$(CARGO) test --all-features

docs:
	$(CARGO) doc --no-deps

clean:
	$(CARGO) clean

install-tools:
	$(CARGO) install cargo-audit cargo-deny cargo-machete jsonschema-cli

validate-examples:
	$(JSONSCHEMA) validate docs/api_design/api_v1.schema.json --instance docs/api_design/api_v1.example.json

update-api-design-svg:
	$(PLANTUML) -tsvg docs/api_design/api_design.plantuml

fmt-json:
	$(JQ) --indent 2 . docs/api_design/api_v1.schema.json > /tmp/schema.tmp && mv /tmp/schema.tmp docs/api_design/api_v1.schema.json
	$(JQ) --indent 2 . docs/api_design/api_v1.example.json > /tmp/example.tmp && mv /tmp/example.tmp docs/api_design/api_v1.example.json
	@echo "JSON files formatted"
