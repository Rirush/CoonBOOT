# CoonBOOT

UEFI x86\_64 bootloader for upcoming CoonOS written in Rust

## Building

To build the bootloader, you'll need **nightly** Rust and `cargo-xbuild` installed. You can install `xbuild` by executing this command:

```
cargo install cargo-xbuild 
```

To build debug binary of the bootloader, use this command:

```
cargo xbuild --target x86_64-unknown-uefi
```

or build release binary using:

```
cargo xbuild --release --target x86_64-unknown-uefi
```

Depending on the type of build, compiled binary can be found in either `target/x86_64-unknown-uefi/debug/coonboot.efi` or `target/x86_64-unknown-uefi/release/coonboot.efi`.

## Running the bootloader

In order to run bootloader, you'll need to prepare EFI partition. As of now, EFI partition must have these folders and files:

```
EFI\
'--> CoonOS\
        '--> Kernel    # Kernel ELF executable
```

This structure is a subject to change until the boot process will be stabilized. If one of the files is missing, bootloader will crash with "check your installation" message.