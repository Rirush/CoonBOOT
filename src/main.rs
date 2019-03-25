#![no_std]
#![no_main]
#![feature(try_trait)]
#![feature(alloc)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use log::*;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileHandle, FileInfo, FileMode, RegularFile};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::*;
use xmas_elf::ElfFile;

/// Bootloader entry point
#[no_mangle]
pub extern "C" fn efi_main(handle: Handle, system_table: SystemTable<Boot>) -> Status {
    let status = main(handle, system_table);
    match status {
        Status::SUCCESS => Status::SUCCESS,
        Status::NOT_FOUND => {
            error!("Bootloader wasn't able to find kernel image.");
            error!("Please make sure that your CoonOS installation is valid and that this bootloader version is compatible with the installed version of OS");
            Status::ABORTED
        }
        _ => {
            error!(
                "Bootloader failed to start operating system. Error {:?}",
                status
            );
            Status::ABORTED
        }
    }
}

fn get_info_size(file: &mut FileHandle) -> Option<usize> {
    let mut buffer: [u8; 0] = [0; 0];
    match file.get_info::<FileInfo>(&mut buffer) {
        Ok(_) => {
            // Never happens, actually
            None
        }
        Err(e) => {
            let (_, code) = e.split();
            code
        }
    }
}

fn main(_handle: Handle, system_table: SystemTable<Boot>) -> Status {
    // Initialize UEFI services first, so we'll have access to output and EFI file system
    if uefi_services::init(&system_table).is_err() {
        // If we fail to initialize services (which should never happen at all), hand over control back to UEFI
        return Status::ABORTED;
    }

    // Be less verbose on release builds
    if !cfg!(debug_assertions) {
        log::set_max_level(log::LevelFilter::Info);
    } else {
        log::set_max_level(log::LevelFilter::Trace);
    }

    // Reset the console
    if let Err(e) = system_table.stdout().reset(false) {
        // Failure in resetting the console isn't fatal, so instead of exiting we just output warning
        warn!("Failed to reset console. Error {:?}", e);
    }

    // Write pretty (kinda) message about which version of CoonBOOT the user is currently running
    info!("CoonBOOT v{}", env!("CARGO_PKG_VERSION"));
    trace!("Boot process start");

    // Request SimpleFileSystem and open EFI volume's root
    let mut file = system_table
        .boot_services()
        .locate_protocol::<SimpleFileSystem>()
        .map(|fs| unsafe { (*fs.log().get()).open_volume() })??
        .log();
    trace!("Acquired EFI root handle");

    // As of now, assume that the kernel is located in /EFI/CoonOS/Kernel, later on it'll be moved onto partition with CoonOS installed
    let mut handle = file
        .handle()
        .open(
            "\\EFI\\CoonOS\\Kernel",
            FileMode::Read,
            FileAttribute::empty(),
        )?
        .log();
    trace!("Opened kernel file successfully");

    // Get size for FileInfo buffer
    let file_info_size = get_info_size(&mut handle).unwrap_or(0);
    if file_info_size == 0 {
        // Abort if we weren't able to get FileInfo size
        return Status::ABORTED;
    }

    trace!(
        "Allocating {} bytes for file information...",
        file_info_size
    );
    let mut buffer: Vec<u8> = vec![0; file_info_size];

    // Get FileInfo structure using buffer we allocated before
    let info = handle
        .get_info::<FileInfo>(&mut buffer)
        .map_err(|e| e.status())?
        .log();

    // Extract size of the file from FileInfo and allocate buffer for kernel
    let size = info.file_size() as usize;
    trace!("Allocating {} bytes for kernel binary...", size);
    let mut file = unsafe { RegularFile::new(handle) };
    let mut buffer: Vec<u8> = vec![0; size];

    // Read the kernel into the buffer
    file.read(&mut buffer).map_err(|e| e.status())?.log();
    trace!("Loaded ELF into memory");

    // Construct ElfFile from loaded ELF
    let _elf = ElfFile::new(&buffer).map_err(|e| {
        error!("Failed to parse ELF. {}", e);
        Status::ABORTED
    })?;
    trace!("Parsed ELF");

    Status::SUCCESS
}
