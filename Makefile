# WATOS Makefile
# Quick access to common build and test operations

.PHONY: all build test boot clean help

# Default target
all: build

# Build the kernel and all applications
build:
	@./scripts/build.sh

# Run automated tests
test:
	@./scripts/test.sh

# Boot in interactive mode (QEMU)
boot:
	@./scripts/boot_test.sh -i

# Run boot test with command
boot-cmd:
	@if [ -z "$(CMD)" ]; then \
		echo "Error: Please specify a command with CMD=..."; \
		echo "Example: make boot-cmd CMD=gwbasic"; \
		exit 1; \
	fi
	@./scripts/boot_test.sh --cmd '$(CMD)'

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	@rm -rf target/
	@rm -rf uefi_test/
	@rm -f uefi_boot.img
	@rm -rf ai-temp/logs/*.log
	@rm -rf ai-temp/logs/*.img
	@echo "Clean complete"

# Show available targets
help:
	@echo "WATOS Makefile - Available targets:"
	@echo ""
	@echo "  make build      - Build kernel and all applications"
	@echo "  make test       - Run automated tests"
	@echo "  make boot       - Boot in interactive mode (QEMU)"
	@echo "  make boot-cmd   - Boot with command (e.g., make boot-cmd CMD=gwbasic)"
	@echo "  make clean      - Clean build artifacts"
	@echo "  make help       - Show this help message"
	@echo ""
	@echo "Examples:"
	@echo "  make               # Build everything (default)"
	@echo "  make boot          # Boot WATOS interactively"
	@echo "  make boot-cmd CMD='ls'"
