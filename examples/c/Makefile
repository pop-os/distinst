TARGET=../../target

install: install.c $(TARGET)/debug/libdistinst.so
	gcc $< -o $@ -I$(TARGET)/include $(TARGET)/debug/libdistinst.so -Wall

clean:
	rm -f install

run: install
	./install
