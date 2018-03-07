TARGET=../../target

LIB=$(TARGET)/release/libdistinst.so

all: install list

install: install.vala $(LIB)
	valac $< -o $@ --save-temps -X -I$(TARGET) -X $(LIB) --vapidir=../../ffi/ --pkg=distinst

list: list.vala $(LIB)
	valac $< -o $@ --save-temps -X -I$(TARGET) -X $(LIB) --vapidir=../../ffi/ --pkg=distinst

clean:
	rm -f install list
