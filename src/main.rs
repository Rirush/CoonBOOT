#![no_std]
#![no_main]

use uefi::*;
use uefi::prelude::*;
use log::*;


/// Bootloader entry point
#[no_mangle]
pub extern "C" fn efi_main(handle: Handle, system_table: SystemTable<Boot>) -> Status {
    // Initialize UEFI services first, so we'll have access to output and EFI file system
    if let Err(_) = uefi_services::init(&system_table) {
        // If we fail to initialize services (which should never happen at all), hand over control back to UEFI
        return Status::ABORTED;
    }

    // Reset the console
    if let Err(_) = system_table.stdout().reset(false) {
        // Failure in resetting the console isn't fatal, so instead of exiting we just output warning
        warn!("Failed to reset console");
    }

    // Write pretty (kinda) message about which version of CoonBOOT the user is currently running
    info!("CoonBOOT v{}", env!("CARGO_PKG_VERSION"));

    // We must return from this function as of now. Later on the control will be handed to the kernel instead of back to UEFI
    return Status::SUCCESS;
}