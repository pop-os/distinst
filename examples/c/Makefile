TARGET=../../target

LIB=$(TARGET)/release/libdistinst.so

all: install

install: install.c $(LIB)
	gcc $< -o $@ -I$(TARGET) $(LIB) -Wall

clean:
	rm -f install
