#![no_std]
#![no_main]

#![feature(try_trait)]
#![feature(alloc)]

extern crate alloc;

use core::ops::Try;
use core::cell::UnsafeCell;
use uefi::*;
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::media::file::{File,FileMode,FileAttribute,FileHandle,FileInfo};
use uefi::prelude::*;
use log::*;
use alloc::vec::Vec;
use alloc::vec;

/// Bootloader entry point
#[no_mangle]
pub extern "C" fn efi_main(_handle: Handle, system_table: SystemTable<Boot>) -> Status {
    // Initialize UEFI services first, so we'll have access to output and EFI file system
    if let Err(_) = uefi_services::init(&system_table) {
        // If we fail to initialize services (which should never happen at all), hand over control back to UEFI
        return Status::ABORTED;
    }

    // Reset the console
    if let Err(e) = system_table.stdout().reset(false) {
        // Failure in resetting the console isn't fatal, so instead of exiting we just output warning
        warn!("Failed to reset console. Error {:?}", e);
    }

    // Write pretty (kinda) message about which version of CoonBOOT the user is currently running
    info!("CoonBOOT v{}", env!("CARGO_PKG_VERSION"));

    // Request SimpleFileSystem protocol from UEFI
    let file_protocol: Result<&UnsafeCell<SimpleFileSystem>, _> = system_table.boot_services().locate_protocol();
    match file_protocol {
        Ok(c) => {
            // Log warning if there's any and then receive underlying SimpleFileSystem from UnsafeCell. Now we're ready to find and load kernel to the memory.
            let file_protocol = c.log().get();
            // Sadly, reference to SimpleFileSystem is a raw pointer, so from here on we have to use unsafe block
            unsafe {
                // Open EFI volume and proceed to loading file
                match (*file_protocol).open_volume() {
                    Ok(file) => {
                        // Again, as before, log all warnings and unwrap Completion
                        let mut file = file.log();
                        // As of now, assume that the kernel is located in /EFI/CoonOS/Kernel, later on it'll be moved onto partition with CoonOS installed
                        match file.handle().open("\\EFI\\CoonOS\\Kernel", FileMode::Read, FileAttribute::empty()) {
                            Ok(file) => {
                                // We're almost done with loading kernel
                                return process_kernel(file.log());
                            },
                            Err(e) => {
                                let s = Status::from_error(e);
                                match s {
                                    Status::NOT_FOUND => {
                                        // We know why this error happened, so output prettier message
                                        error!("No kernel found, please make sure CoonOS is installed and that this bootloader is compatible with installed version of OS.");
                                        return Status::ABORTED;
                                    },
                                    _ => {
                                        error!("Failed to open kernel file. Error {:?}", s);
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to open ESP volume. Error {:?}", e);
                        return Status::ABORTED;
                    }
                }
            }
        },
        Err(e) => {
            // Since we need access to file system, we are totally unable to continue boot process
            error!("Failed to retrieve SimpleFileSystem protocol, unable to continue. Error {:?}", e);
            return Status::ABORTED;
        }
    }

    // We must return from this function as of now. Later on the control will be handed to the kernel instead of back to UEFI
    return Status::SUCCESS;
}

// Since efi_main() gets too bloated, this function loads kernel to memory and gives control to it
fn process_kernel(mut f: FileHandle) -> Status {
    // Create buffer with length of zero, so we'll be able to get required size of the buffer for FileInfo struct
    let mut buffer: [u8; 0] = [0;0];
    // This always succeeds because 0 is always not enough for FileInfo to fit
    if let Err(e) = f.get_info::<FileInfo>(&mut buffer) {
        let (s, size) = e.split();
        match s {
            Status::BUFFER_TOO_SMALL => {
                match size {
                    Some(size) => {
                        info!("Allocating {} bytes for file information...", size);
                        let mut buffer: Vec<u8> = vec![0; size];
                        match f.get_info::<FileInfo>(&mut buffer) {
                            Ok(info) => {
                                let size = info.log().file_size();
                                info!("Allocating {} bytes for kernel binary...", size);
                            },
                            Err(e) => {
                                error!("Failed to get information about file. Error {:?}", e);
                                return Status::ABORTED;
                            }
                        }
                    },
                    None => {
                        // Not sure that this can happen, but this won't do any bad anyway
                        error!("Can't allocate buffer for file information. Buffer size is unknown");
                        return Status::ABORTED;
                    }
                }
            },
            _ => {
                // Unexpected error occured, so we have to exit
                error!("Failed to get information about file. Error {:?}", s);
                return Status::ABORTED;
            }
        }
    }
    return Status::SUCCESS;
}