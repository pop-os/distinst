TARGET=../../target

install: install.vala $(TARGET)/release/libdistinst.so
	valac $< -o $@ -X -I$(TARGET)/include -X $(TARGET)/release/libdistinst.so --vapidir=../../src/ --pkg=distinst

clean:
	rm -f install

run: install
	./install
