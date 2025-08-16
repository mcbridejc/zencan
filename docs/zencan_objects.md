# Zencan Objects

There are some standard objects in zencan, which may be included in the application's object
dictionary. Some are always included, some are optional and depend on the device configuration file
as to whether they are included or not.

All of these objects are have indices in the range of 0x5000 to 0x5FFFF.

## Application Configuration

### 0x5000 Autostart

## Bootloader

### 0x5500 Bootloader Info

All applications which support the bootloader must implement this record object. It will specify
whether the device supports bootloader operations, and if so which ones. The bootloader protocol can
be supported both by an application and by a bootloader. Some sections may be programmable only
while in the bootloader, which means that the device must be rebooted to bootloader mode before they
can be programmed -- this is typically the case for application reprogramming. Other sections may be
programmable in application mode.


| Index | Data Type | Access Type | Description                 |
| ----- | --------- | ----------- | --------------------------- |
| 0     | u8        | ro          | Highest sub index           |
| 1     | u32       | ro          | Bootloader config           |
| 2     | u8        | ro          | Number of loadable section  |
| 3     | u32       | wo          | Reset to bootloader command |
|       |           |             |                             |


#### Sub 1 Bootloader config

| Bits | Description                                                       |
| ---- | ----------------------------------------------------------------- |
| 0    | 0 - bootloader not supported, 1 - Bootloader supported            |
| 1    | 0 - unable to reset to bootlader, 1 - Able to reset to bootloader |
| 2-3  | 0 - Application mode, 1 - reserved, 2 - Bootloader, 3 - reserved  |
|      |                                                                   |

#### Sub 2 Number of loadable sections

Specifies how many bootloader sections are present.

#### Sub 3 Reset Command

Writing the correct value to this sub object resets the application into bootloader mode, if it is
supported. The application will send an Abort in response to a write to this sub object if it does
not support reset (e.g., because it does not have a bootloader, or because it is already running the
bootloader).

The reset command value is 0x544F4F42, or 'BOOT'.

### 0x5510 - 0x551f Bootloader Sections

Used to describe and control bootloader sections. A bootloadable device must define 1 to 16 loadable
sections, each section being individually erasable and re-programmable. The number of sections is
specified in object 0x5000sub2.

Each section is a record object, with the following fields:

| Index | Data Type     | Access Type | Description                              |
| ----- | ------------- | ----------- | ---------------------------------------- |
| 0     | u8            | const       | Highest sub index                        |
| 1     | u8            | const       | Mode bits. Bit 0: currently programmable |
| 2     | VisibleString | const       | Section name                             |
| 3     | u32           | wo          | Erase Command                            |
| 4     | Domain        | wo          | Programming Data                         |

#### Sub 1 Mode Bits

Indicates if the section can be programmed. Some sections may only be programmable when the device
is in bootloader mode.

#### Sub 2 Visible String

A name for the section.

#### Sub 3 Size

The number of bytes available to write in this section.

#### Sub 4 Erase Commmand

Writing the correct value to this object triggers an erase of the section, making it available for
programming.

The command code is 0x53415245, corresponding to the ascii characters ERAS (little endian)

#### Sub 5 Programming Data Domain

This is a domain object which is written to program. It can only be written after an erase command
has been successfully issued.



