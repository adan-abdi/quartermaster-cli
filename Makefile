# Quartermaster CLI Makefile

.PHONY: help build install clean test run qm quartermaster

# Default target
help:
	@echo "Quartermaster CLI - Navigate the constellations of your codebase"
	@echo ""
	@echo "Available commands:"
	@echo "  make build        - Build the CLI in release mode"
	@echo "  make install      - Install the CLI globally"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make test         - Run tests"
	@echo "  make run          - Run the CLI (quartermaster)"
	@echo "  make qm           - Run the CLI (qm alias)"
	@echo "  make quartermaster - Run the CLI (full name)"
	@echo "  make dev          - Build and run in development mode"
	@echo "  make docs         - Generate a local .quartermaster workspace for this repo"
	@echo ""
	@echo "Usage examples:"
	@echo "  quartermaster chart ./my-project"
	@echo "  qm analyze github.com/rust-lang/rust"
	@echo "  quartermaster chart . --no-open --non-interactive"

# Build the CLI in release mode
build:
	@echo "🔨 Building Quartermaster CLI..."
	cargo build --release
	@echo "✅ Build complete! Binary available at target/release/quartermaster"

# Install the CLI globally
install: build
	@echo "📦 Installing Quartermaster CLI..."
	cargo install --path .
	@echo "✅ Installation complete! Run 'quartermaster' or 'qm' to use."

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean
	@echo "✅ Clean complete!"

# Run tests
test:
	@echo "🧪 Running tests..."
	cargo test
	@echo "✅ Tests complete!"

# Run the CLI (quartermaster)
run: quartermaster

# Run the CLI (qm alias)
qm:
	@echo "🚀 Running Quartermaster CLI (qm)..."
	cargo run --bin qm

# Run the CLI (full name)
quartermaster:
	@echo "🚀 Running Quartermaster CLI..."
	cargo run --bin quartermaster

# Development build and run
dev:
	@echo "🔧 Development mode - Building and running..."
	cargo run --bin quartermaster

# Generate documentation for this repo
docs:
	@echo "📚 Generating Quartermaster workspace for this repository..."
	cargo run --bin quartermaster -- chart .. --no-open --non-interactive
	@echo "✅ Workspace generated in ../.quartermaster/"

# Build both binaries
build-all:
	@echo "🔨 Building both quartermaster and qm binaries..."
	cargo build --release --bins
	@echo "✅ Build complete! Binaries available at:"
	@echo "  - target/release/quartermaster"
	@echo "  - target/release/qm"

# Quick development commands
dev-qm:
	@echo "🚀 Quick development run (qm)..."
	cargo run --bin qm

dev-full:
	@echo "🚀 Quick development run (quartermaster)..."
	cargo run --bin quartermaster

# Check code formatting
fmt:
	@echo "🎨 Checking code formatting..."
	cargo fmt --check

# Format code
fmt-fix:
	@echo "🎨 Formatting code..."
	cargo fmt

# Run linter
clippy:
	@echo "🔍 Running clippy linter..."
	cargo clippy -- -D warnings

# Full CI check
ci: fmt clippy test
	@echo "✅ All CI checks passed!"

# Show version
version:
	@echo "📋 Quartermaster CLI version:"
	cargo run --bin quartermaster -- --version

# Show help
show-help:
	@echo "📋 Quartermaster CLI help:"
	cargo run --bin quartermaster -- --help
