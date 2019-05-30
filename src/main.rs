#![no_std]
#![no_main]
#![feature(try_trait)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use log::*;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileHandle, FileInfo, FileMode, RegularFile};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::MemoryDescriptor;
use uefi::table::cfg;
use uefi::*;
use xmas_elf::program::ProgramHeader::Ph64;
use xmas_elf::program::ProgramHeader64;
use xmas_elf::program::Type::Load;
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

#[repr(C)]
struct SystemDescription {
    pub acpi2_address: usize,
    pub smbios3_address: usize,
    pub memory_map: Vec<MemoryDescriptor>,
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

    // Create SystemDescription structure. It will be passed to kernel later on
    let mut system_description_table = SystemDescription {
        acpi2_address: 0,
        smbios3_address: 0,
        memory_map: vec![],
    };

    // Read necessary values into SystemDescription structure
    trace!("Reading configuration table");
    for entry in system_table.config_table() {
        match entry.guid {
            cfg::ACPI2_GUID => system_description_table.acpi2_address = entry.address as usize,
            cfg::SMBIOS_GUID => system_description_table.smbios3_address = entry.address as usize,
            _ => {}
        }
    }
    trace!("Configuration table read finished");

    // If we failed to find ACPI and SMBIOS tables - abort boot process
    if system_description_table.acpi2_address == 0 || system_description_table.smbios3_address == 0
    {
        error!("This system doesn't support ACPIv2 or SMBIOS. Unable to boot");
        return Status::ABORTED;
    }

    // Save memory table in SystemDescription structure
    trace!("Reading memory table");
    let mmap_size = system_table.boot_services().memory_map_size();
    let mut mmap_buffer: Vec<u8> = vec![0; mmap_size];
    let (_key, iter) = system_table
        .boot_services()
        .memory_map(&mut mmap_buffer)?
        .log();
    for entry in iter {
        trace!(
            "{:?}: PMem 0x{:x} Pages {}",
            entry.ty,
            entry.phys_start,
            entry.page_count
        );
        system_description_table.memory_map.push(*entry);
    }
    trace!("Memory table read finished");

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
    let elf = ElfFile::new(&buffer).map_err(|e| {
        error!("Failed to parse ELF. {}", e);
        Status::ABORTED
    })?;
    trace!("Parsed ELF");

    // Create vector for storing segments, that are required to load
    let mut segments: Vec<ProgramHeader64> = vec![];

    // Iterate over all segments in this ELF file and check which ones we need to load
    for segment in elf.program_iter() {
        if let Ph64(header) = segment {
            if Load
                == header.get_type().map_err(|e| {
                    error!("Failed to get segment type. {}", e);
                    Status::ABORTED
                })?
            {
                segments.push(*header);
            }
        }
    }
    trace!("Retrieved list of segments to load");

    // List all segments if we're running in debug mode
    if cfg!(debug_assertions) {
        for segment in segments {
            trace!(
                "Offset 0x{:x} -> VMem 0x{:x} : Size 0x{:x}",
                segment.offset,
                segment.virtual_addr,
                segment.mem_size
            );
        }
    }

    Status::SUCCESS
}
