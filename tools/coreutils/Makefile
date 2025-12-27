# Makefile for WATOS Core Utilities
# Builds DOS .COM files from assembly source

# Assembler
AS = nasm
ASFLAGS = -f bin -O0

# Output directory
OUTDIR = ../../rootfs/BIN

# Source files
SOURCES = $(wildcard *.asm)
TARGETS = $(patsubst %.asm,$(OUTDIR)/%.COM,$(SOURCES))

# Default target
all: $(OUTDIR) $(TARGETS)

# Create output directory
$(OUTDIR):
	mkdir -p $(OUTDIR)

# Build .COM files from .asm
$(OUTDIR)/%.COM: %.asm
	$(AS) $(ASFLAGS) -o $@ $<
	@echo "Built $@"

# Clean target
clean:
	rm -f $(TARGETS)

# List targets
list:
	@echo "Targets:"
	@echo "$(TARGETS)" | tr ' ' '\n'

.PHONY: all clean list
