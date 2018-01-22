TARGET=../../target

all: install list

debug: install-debug list-debug

install: install.c $(TARGET)/release/libdistinst.so
	gcc $< -o $@ -I$(TARGET)/include $(TARGET)/release/libdistinst.so -Wall

install-debug: install.c $(TARGET)/debug/libdistinst.so
	gcc $< -o $@ -I$(TARGET)/include $(TARGET)/debug/libdistinst.so -Wall

list: list.c $(TARGET)/release/libdistinst.so
	gcc $< -o $@ -I$(TARGET)/include $(TARGET)/release/libdistinst.so -Wall

list-debug: list.c $(TARGET)/debug/libdistinst.so
	gcc $< -o $@ -I$(TARGET)/include $(TARGET)/debug/libdistinst.so -Wall

clean:
	rm -f install install-debug list list-debug