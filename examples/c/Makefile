TARGET=../../target

install: install.c $(TARGET)/release/libdistinst.so
	gcc $< -o $@ -I$(TARGET)/include $(TARGET)/release/libdistinst.so -Wall

clean:
	rm -f install

run: install
	./install
