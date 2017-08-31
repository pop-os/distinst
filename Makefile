prefix ?= /usr/local
exec_prefix ?= $(prefix)
libdir ?= $(exec_prefix)/lib
includedir ?= $(prefix)/include
datarootdir ?= $(prefix)/share
datadir ?= $(datarootdir)

TARGETS=\
	target/release/libdistinst.so \
	target/include/distinst.h \
	target/pkgconfig/distinst.pc

.PHONY: all clean distclean install uninstall update

all: $(TARGETS)

clean:
	cargo clean

distclean: clean
	rm -f Cargo.lock

install: $(TARGETS)
	install -D -m 0644 "target/release/libdistinst.so" "$(DESTDIR)$(libdir)/libdistinst.so"
	install -D -m 0644 "target/include/distinst.h" "$(DESTDIR)$(includedir)/distinst.h"
	install -D -m 0644 "target/pkgconfig/distinst.pc" "$(DESTDIR)$(datadir)/pkgconfig/distinst.pc"

uninstall:
	rm -f "$(DESTDIR)$(libdir)/libdistinst.so"
	rm -f "$(DESTDIR)$(includedir)/distinst.h"
	rm -f "$(DESTDIR)$(datadir)/pkgconfig/distinst.pc"

update:
	cargo update

$(TARGETS):
	cargo build --release
