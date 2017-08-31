# distinst

Distribution Installer Backend

## Build Instructions

In order to build `distinst` on Ubuntu, you will need to follow these instructions:

```
# Install Rust
curl https://sh.rustup.rs -sSf | sh

# Build in release mode
cargo build --release
```

The resulting library is at `target/release/libdistinst.so`, and the header is at `target/include/distinst.h`
