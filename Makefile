prefix ?= /usr/local
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datarootdir = $(prefix)/share
datadir = $(datarootdir)
RELEASE = debug

SRC=Cargo.toml $(shell find src crates -type f -wholename '*src/*.rs' \
	-o -name 'Cargo.toml' \
	-o -name 'Cargo.lock')
CLI_SRC=cli/Cargo.toml $(shell find cli/src -type f -name '*.rs')
FFI_SRC=ffi/Cargo.toml ffi/build.rs $(shell find ffi/src -type f -name '*.rs')
PACKAGE=distinst

HEADER=target/$(PACKAGE).h
PKGCONFIG=target/$(PACKAGE).pc
VAPI=ffi/$(PACKAGE).vapi

DEBUG ?= 0
ifeq (0,$(DEBUG))
	ARGSD += --release
	RELEASE = release
endif

VENDORED ?= 0
ifneq ($(VENDORED),0)
	ARGS += "--frozen"
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

tests: $(SRC)
	cargo test $(ARGS)
	for crate in crates/*; do \
		cargo test $(ARGS) --manifest-path $$crate/Cargo.toml; \
	done

$(BINARY): $(SRC) $(CLI_SRC)
	cargo build --manifest-path cli/Cargo.toml $(ARGS) $(ARGSD)

$(LIBRARY) $(HEADER) $(PKGCONFIG).stub: $(SRC) $(FFI_SRC)
	cargo build --manifest-path ffi/Cargo.toml $(ARGS) $(ARGSD)

$(PKGCONFIG): $(PKGCONFIG).stub
	echo "libdir=$(libdir)" > "$@.partial"
	echo "includedir=$(includedir)" >> "$@.partial"
	cat "$<" >> "$@.partial"
	mv "$@.partial" "$@"
