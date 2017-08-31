PREFIX ?= /usr

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
	install -D -m 0644 target/release/libdistinst.so "$(DESTDIR)$(PREFIX)/lib/libdistinst.so"
	install -D -m 0644 target/include/distinst.h "$(DESTDIR)$(PREFIX)/include/distinst.h"
	install -D -m 0644 target/pkgconfig/distinst.pc "$(DESTDIR)$(PREFIX)/share/pkgconfig/distinst.pc"

uninstall:
	rm -f "$(DESTDIR)$(PREFIX)/lib/libdistinst.so"
	rm -f "$(DESTDIR)$(PREFIX)/include/distinst.h"
	rm -f "$(DESTDIR)$(PREFIX)/share/pkgconfig/distinst.pc"

update:
	cargo update

$(TARGETS):
	cargo build --release
