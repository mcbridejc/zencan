//! Constants defining standard object and special values
//!
//!

/// Object indices for standard objects
pub mod object_ids {
    /// The Device Name object index
    pub const DEVICE_NAME: u16 = 0x1008;
    /// The hardware version object index
    pub const HARDWARE_VERSION: u16 = 0x1009;
    /// Save objects command object index
    pub const SAVE_OBJECTS: u16 = 0x1010;
    /// The software version object index
    pub const SOFTWARE_VERSION: u16 = 0x100A;
    /// The heartbeat producer time object index
    pub const HEARTBEAT_PRODUCER_TIME: u16 = 0x1017;
    /// The identity object index
    pub const IDENTITY: u16 = 0x1018;

    /// The first RPDO communication parameter index. RPDO comm can be stored from 0x1400 to 0x15FF.
    pub const RPDO_COMM_BASE: u16 = 0x1400;
    ///  The first RPDO mapping parameter index. RPDO mappings can be stored from 0x1600 to 0x17FF;
    pub const RPDO_MAP_BASE: u16 = 0x1600;
    /// The first TPDO communication parameter index. TPDO comms can be stored from 0x1800 to 0x19FF.
    pub const TPDO_COMM_BASE: u16 = 0x1800;
    ///  The first TPDO mapping parameter index. TPDO mappings can be stored from 0x1A00 to 0x1BFF;
    pub const TPDO_MAP_BASE: u16 = 0x1A00;

    /// The auto start object index
    pub const AUTO_START: u16 = 0x5000;
}

/// Special values used to access standard objects
pub mod values {
    /// Magic value used to trigger object storage by writing to object 0x1010
    pub const SAVE_CMD: u32 = 0x73617665;

    /// Magic value used to trigger a reset to bootloader by writing to object 0x5500
    pub const BOOTLOADER_RESET_CMD: u32 = 0x544F4F42;

    /// Magic value used to trigger bootloader section erase by writing objects 0x5510-0x551f
    pub const BOOTLOADER_ERASE_CMD: u32 = 0x53415245;
}
