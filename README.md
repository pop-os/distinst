# distinst

Distribution Installer Backend

## Build Instructions

In order to build `distinst` on Ubuntu, you will need to follow these instructions:

```
# Install Rust
curl https://sh.rustup.rs -sSf | sh

# Build in release mode
make

# Install
sudo make install

# Uninstall
sudo make uninstall
```

The following files will be generated:
Library: `target/release/libdistinst.so`
Header: `target/include/distinst.h`
pkg-config: `target/pkg-config/distinst.pc`
