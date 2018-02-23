TARGET=../../target

LIB=$(TARGET)/debug/libdistinst.so

install: install.vala $(LIB)
	valac $< -o $@ -X -I$(TARGET)/include -X $(LIB) --vapidir=../../src/ --pkg=distinst

list: list.vala $(TARGET)/release/libdistinst.so
	valac $< -o $@ -X -I$(TARGET)/include -X $(LIB) --vapidir=../../src/ --pkg=distinst

clean:
	rm -f install

run: install
	./install
