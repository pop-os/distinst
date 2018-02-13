TARGET=../../target

all: install

debug: install-debug

install: install.c $(TARGET)/release/libdistinst.so
	gcc $< -o $@ -I$(TARGET)/include $(TARGET)/release/libdistinst.so -Wall

install-debug: install.c $(TARGET)/debug/libdistinst.so
	gcc $< -o $@ -I$(TARGET)/include $(TARGET)/debug/libdistinst.so -Wall

clean:
	rm -f install install-debug
