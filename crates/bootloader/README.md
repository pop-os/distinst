# distinst-bootloader

Detect whether a Linux system is in EFI or BIOS mode.

```rust,no_exec
extern crate distinst_bootloader;
use distinst_bootloader::Bootloader;

match Bootloader::detect() {
    Bootloader::Efi => println!("System is in EFI mode"),
    Bootloader::Bios => println!("System is in BIOS mode")
}
```
