prefix ?= /usr/local
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datarootdir = $(prefix)/share
datadir = $(datarootdir)

.PHONY: all clean distclean install uninstall update

BIN=distinst

all: target/release/$(BIN) target/release/libdistinst.so target/include/distinst.h target/pkgconfig/distinst.pc

clean:
	cargo clean

distclean: clean
	rm -rf .cargo vendor

install: all
	install -D -m 0755 "target/release/$(BIN)" "$(DESTDIR)$(bindir)/$(BIN)"
	install -D -m 0644 "target/release/libdistinst.so" "$(DESTDIR)$(libdir)/libdistinst.so"
	install -D -m 0644 "target/include/distinst.h" "$(DESTDIR)$(includedir)/distinst.h"
	install -D -m 0644 "target/pkgconfig/distinst.pc" "$(DESTDIR)$(datadir)/pkgconfig/distinst.pc"
	install -D -m 0644 "src/distinst.vapi" "$(DESTDIR)$(datadir)/vala/vapi/distinst.vapi"

uninstall:
	rm -f "$(DESTDIR)$(bindir)/$(BIN)"
	rm -f "$(DESTDIR)$(libdir)/libdistinst.so"
	rm -f "$(DESTDIR)$(includedir)/distinst.h"
	rm -f "$(DESTDIR)$(datadir)/pkgconfig/distinst.pc"
	rm -f "$(DESTDIR)$(datadir)/vala/vapi/distinst.vapi"

update:
	cargo update

.cargo/config: vendor_config
	mkdir -p .cargo
	cp $< $@

vendor: .cargo/config
	cargo vendor
	touch vendor

target/release/$(BIN) target/release/libdistinst.so target/include/distinst.h target/pkgconfig/distinst.pc.stub:
	if [ -d vendor ]; \
	then \
		cargo build --release --frozen; \
	else \
		cargo build --release; \
	fi

target/pkgconfig/distinst.pc: target/pkgconfig/distinst.pc.stub
	echo "libdir=$(libdir)" > "$@.partial"
	echo "includedir=$(includedir)" >> "$@.partial"
	cat "$<" >> "$@.partial"
	mv "$@.partial" "$@"
