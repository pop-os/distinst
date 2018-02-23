TARGET=../../target

LIB=$(TARGET)/release/libdistinst.so

install: install.vala $(LIB)
	valac $< -o $@ --save-temps -X -I$(TARGET)/include -X $(LIB) --vapidir=../../ffi/ --pkg=distinst

list: list.vala $(LIB)
	valac $< -o $@ --save-temps -X -I$(TARGET)/include -X $(LIB) --vapidir=../../ffi/ --pkg=distinst

clean:
	rm -f install list
