# Makefile for WATOS Core Utilities
# Builds native 64-bit Rust binaries (ELF64)

# Rust build settings
CARGO = cargo +nightly
TARGET = x86_64-unknown-none
RELEASE_FLAGS = -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem --release --target $(TARGET)

# Output directories
OUTDIR = ../../rootfs/BIN
TARGET_DIR = target/$(TARGET)/release

# Utilities to build
UTILS = echo cat copy del ren

# Default target
all: $(addprefix $(OUTDIR)/,$(addsuffix .EXE,$(UTILS)))

# Build a utility
$(OUTDIR)/%.EXE:
	@mkdir -p $(OUTDIR)
	$(CARGO) build $(RELEASE_FLAGS) --bin $*
	cp $(TARGET_DIR)/$* $@
	@echo "Built $@"

# Clean
clean:
	$(CARGO) clean
	rm -f $(OUTDIR)/*.EXE

# Help
help:
	@echo "WATOS Core Utilities Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all       - Build all utilities (default)"
	@echo "  clean     - Remove build artifacts"
	@echo "  help      - Show this help"
	@echo ""
	@echo "Utilities:"
	@echo "  $(UTILS)"

.PHONY: all clean help
