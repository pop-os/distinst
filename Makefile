prefix ?= /usr/local
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datarootdir = $(prefix)/share
datadir = $(datarootdir)

.PHONY: all clean distclean install uninstall update

SRC=Cargo.toml src/* src/*/*
FFI_SRC=ffi/Cargo.toml ffi/build.rs ffi/src/*

PACKAGE=distinst

BINARY=target/release/$(PACKAGE)
LIBRARY=target/release/lib$(PACKAGE).so
HEADER=target/$(PACKAGE).h
PKGCONFIG=target/$(PACKAGE).pc
VAPI=ffi/$(PACKAGE).vapi

all: $(BINARY) $(LIBRARY) $(HEADER) $(PKGCONFIG)

clean:
	cargo clean
	cargo clean --manifest-path ffi/Cargo.toml

distclean: clean
	rm -rf .cargo vendor

install: all
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

vendor: .cargo/config
	cargo vendor
	touch vendor

$(BINARY): $(SRC)
	if [ -d vendor ]; \
	then \
		cargo build --frozen --manifest-path cli/Cargo.toml --release; \
	else \
		cargo build --manifest-path cli/Cargo.toml --release; \
	fi

$(LIBRARY) $(HEADER) $(PKGCONFIG).stub: $(FFI_SRC)
	if [ -d vendor ]; \
	then \
		cargo build --manifest-path ffi/Cargo.toml --frozen --lib --release; \
	else \
		cargo build --manifest-path ffi/Cargo.toml --lib --release; \
	fi

$(PKGCONFIG): $(PKGCONFIG).stub
	echo "libdir=$(libdir)" > "$@.partial"
	echo "includedir=$(includedir)" >> "$@.partial"
	cat "$<" >> "$@.partial"
	mv "$@.partial" "$@"
