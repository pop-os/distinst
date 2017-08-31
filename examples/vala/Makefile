TARGET=../../target

install: install.vala $(TARGET)/debug/libdistinst.so
	valac $< -o $@ -X -I$(TARGET)/include -X $(TARGET)/debug/libdistinst.so --vapidir=../../src/ --pkg=distinst

clean:
	rm -f install

run: install
	./install
