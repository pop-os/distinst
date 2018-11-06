prefix ?= /usr/local
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datarootdir = $(prefix)/share
datadir = $(datarootdir)
RELEASE = debug

ifndef DEBUG
  ARGS += --release
  RELEASE = release
endif

SRC=Cargo.toml src/* src/*/*
FFI_SRC=ffi/Cargo.toml ffi/build.rs ffi/src/*
PACKAGE=distinst

HEADER=target/$(PACKAGE).h
PKGCONFIG=target/$(PACKAGE).pc
VAPI=ffi/$(PACKAGE).vapi

DEBUG ?= 0
VENDORED = 0

ifeq (0,$(DEBUG))
	ARGSD += --release
	RELEASE = release
endif

ifneq ($(wildcard vendor.tar.xz),)
	VENDORED = 1
	ARGS += --frozen
endif

BINARY=target/$(RELEASE)/$(PACKAGE)
LIBRARY=target/$(RELEASE)/lib$(PACKAGE).so

.PHONY: all clean distclean install uninstall update

all: $(BINARY) $(LIBRARY) $(HEADER) $(PKGCONFIG)

clean:
	cargo clean
	cargo clean --manifest-path ffi/Cargo.toml

distclean: clean
	rm -rf .cargo vendor

install:
	install -D -m 0755 "$(BINARY)" "$(DESTDIR)$(bindir)/$(PACKAGE)"
	install -D -m 0644 "$(LIBRARY)" "$(DESTDIR)$(libdir)/lib$(PACKAGE).so"
	install -D -m 0644 "$(HEADER)" "$(DESTDIR)$(includedir)/$(PACKAGE).h"
	install -D -m 0644 "$(PKGCONFIG)" "$(DESTDIR)$(libdir)/pkgconfig/$(PACKAGE).pc"
	install -D -m 0644 "$(VAPI)" "$(DESTDIR)$(datadir)/vala/vapi/$(PACKAGE).vapi"

uninstall:
	rm -f "$(DESTDIR)$(bindir)/$(PACKAGE)"
	rm -f "$(DESTDIR)$(libdir)/lib$(PACKAGE).so"
	rm -f "$(DESTDIR)$(includedir)/$(PACKAGE).h"
	rm -f "$(DESTDIR)$(libdir)/pkgconfig/$(PACKAGE).pc"
	rm -f "$(DESTDIR)$(datadir)/vala/vapi/$(PACKAGE).vapi"

update:
	cargo update

.cargo/config: vendor_config
	mkdir -p .cargo
	cp $< $@

vendor.tar.xz:
	cargo vendor
	tar pcfJ vendor.tar.xz vendor
	rm -rf vendor

extract:
ifeq (1,$(VENDORED)$(wildcard vendor))
	tar pxf vendor.tar.xz
endif

vendor: .cargo/config vendor.tar.xz

<<<<<<< HEAD
tests: extract $(SRC)
	cargo test $(ARGS)
	for crate in crates/*; do \
		cargo test $(ARGS) --manifest-path $$crate/Cargo.toml; \
	done

$(BINARY): extract $(SRC)
	cargo build --manifest-path cli/Cargo.toml $(ARGS) $(ARGSD)

$(LIBRARY) $(HEADER) $(PKGCONFIG).stub: extract $(FFI_SRC)
	cargo build --manifest-path ffi/Cargo.toml $(ARGS) $(ARGSD)
=======
tests:
	cargo test

$(BINARY): $(SRC)
	if [ -f vendor.tar.xz ]; \
	then \
		tar pxf vendor.tar.xz; \
		cargo build --frozen --manifest-path cli/Cargo.toml $(ARGS); \
	else \
		cargo build --manifest-path cli/Cargo.toml $(ARGS); \
	fi

$(LIBRARY) $(HEADER) $(PKGCONFIG).stub: $(FFI_SRC)
	if [ -d vendor ]; \
	then \
		cargo build --manifest-path ffi/Cargo.toml --frozen --lib $(ARGS); \
	else \
		cargo build --manifest-path ffi/Cargo.toml --lib $(ARGS); \
	fi
>>>>>>> 0e193dc556e764b975613f42b0fbe1e4287bd97e

$(PKGCONFIG): $(PKGCONFIG).stub
	echo "libdir=$(libdir)" > "$@.partial"
	echo "includedir=$(includedir)" >> "$@.partial"
	cat "$<" >> "$@.partial"
	mv "$@.partial" "$@"
