prefix ?= /usr/local
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datarootdir = $(prefix)/share
datadir = $(datarootdir)

.PHONY: all clean distclean install uninstall update

BIN=distinst

all: target/release/$(BIN) target/release/lib$(BIN).so target/include/$(BIN).h target/pkgconfig/$(BIN).pc

debug: target/debug/$(BIN) target/debug/lib$(BIN).so target/include/$(BIN).h target/pkgconfig/$(BIN).pc

clean:
	cargo clean

distclean: clean
	rm -rf .cargo vendor

install: all
	install -D -m 0755 "target/release/$(BIN)" "$(DESTDIR)$(bindir)/$(BIN)"
	install -D -m 0644 "target/release/lib$(BIN).so" "$(DESTDIR)$(libdir)/lib$(BIN).so"
	install -D -m 0644 "target/include/$(BIN).h" "$(DESTDIR)$(includedir)/$(BIN).h"
	install -D -m 0644 "target/pkgconfig/$(BIN).pc" "$(DESTDIR)$(datadir)/pkgconfig/$(BIN).pc"
	install -D -m 0644 "src/$(BIN).vapi" "$(DESTDIR)$(datadir)/vala/vapi/$(BIN).vapi"

uninstall:
	rm -f "$(DESTDIR)$(bindir)/$(BIN)"
	rm -f "$(DESTDIR)$(libdir)/lib$(BIN).so"
	rm -f "$(DESTDIR)$(includedir)/$(BIN).h"
	rm -f "$(DESTDIR)$(datadir)/pkgconfig/$(BIN).pc"
	rm -f "$(DESTDIR)$(datadir)/vala/vapi/$(BIN).vapi"

update:
	cargo update

.cargo/config: vendor_config
	mkdir -p .cargo
	cp $< $@

vendor: .cargo/config
	cargo vendor
	touch vendor

# Each lib crate type has to be built independently, else there will be a compiler error.
target/release/$(BIN) target/release/lib$(BIN).so target/include/$(BIN).h target/pkgconfig/$(BIN).pc.stub:
	if [ -d vendor ]; \
	then \
	    cargo rustc --lib --release -- --crate-type=dylib; \
		cargo build --bin distinst --release; \
	else \
	    cargo rustc --lib --release -- --crate-type=dylib; \
		cargo build --bin distinst --release; \
	fi

target/pkgconfig/$(BIN).pc: target/pkgconfig/$(BIN).pc.stub
	echo "libdir=$(libdir)" > "$@.partial"
	echo "includedir=$(includedir)" >> "$@.partial"
	cat "$<" >> "$@.partial"
	mv "$@.partial" "$@"
