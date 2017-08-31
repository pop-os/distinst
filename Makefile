prefix ?= /usr/local
exec_prefix ?= $(prefix)

TARGETS=\
	target/release/libdistinst.so \
	target/pkgconfig/distinst.pc \
	target/include/distinst.h

.PHONY: all clean distclean install uninstall update

all: $(TARGETS)

clean:
	cargo clean

distclean: clean
	rm -f Cargo.lock

install: $(TARGETS)
	install -D -m 0644 "target/release/libdistinst.so" "$(DESTDIR)$(exec_prefix)/lib/libdistinst.so"
	install -D -m 0644 "target/pkgconfig/distinst.pc" "$(DESTDIR)$(prefix)/lib/pkgconfig/distinst.pc"
	install -D -m 0644 "target/include/distinst.h" "$(DESTDIR)$(prefix)/include/distinst.h"

uninstall:
	rm -f "$(DESTDIR)$(exec_prefix)/lib/libdistinst.so"
	rm -f "$(DESTDIR)$(prefix)/lib/pkgconfig/distinst.pc"
	rm -f "$(DESTDIR)$(prefix)/include/distinst.h"

update:
	cargo update

$(TARGETS):
	cargo build --release
