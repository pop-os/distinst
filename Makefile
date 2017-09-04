prefix ?= /usr/local
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datarootdir = $(prefix)/share
datadir = $(datarootdir)

.PHONY: all clean distclean install uninstall update vendor

all: target/release/distinst target/release/libdistinst.so target/include/distinst.h target/pkgconfig/distinst.pc

clean:
	cargo clean

distclean: clean

install: all
	install -D -m 0755 "target/release/distinst" "$(DESTDIR)$(bindir)/distinst"
	install -D -m 0644 "target/release/libdistinst.so" "$(DESTDIR)$(libdir)/libdistinst.so"
	install -D -m 0644 "target/include/distinst.h" "$(DESTDIR)$(includedir)/distinst.h"
	install -D -m 0644 "target/pkgconfig/distinst.pc" "$(DESTDIR)$(datadir)/pkgconfig/distinst.pc"

uninstall:
	rm -f "$(DESTDIR)$(bindir)/distinst"
	rm -f "$(DESTDIR)$(libdir)/libdistinst.so"
	rm -f "$(DESTDIR)$(includedir)/distinst.h"
	rm -f "$(DESTDIR)$(datadir)/pkgconfig/distinst.pc"

update:
	cargo update

vendor:
	cargo vendor

target/release/distinst target/release/libdistinst.so target/include/distinst.h target/pkgconfig/distinst.pc.stub:
	cargo build --frozen --release

target/pkgconfig/distinst.pc: target/pkgconfig/distinst.pc.stub
	echo "libdir=$(libdir)" > "$@.partial"
	echo "includedir=$(includedir)" >> "$@.partial"
	cat "$<" >> "$@.partial"
	mv "$@.partial" "$@"
