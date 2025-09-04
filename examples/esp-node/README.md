This is a dummy CANopen node that runs on an ESP32C3 microcontroller.

# Prerequisite Tools

To flash the microcontroller through a USB-UART adapter the `espflash` utility is used as defined in `.cargo/config.toml`.
Install it with:
```bash
cargo install cargo-espflash --locked
```

You can check that the target device is detected:
```bash
espflash board-info
```

# Build and Run
Build the firmware and flash it to the target:
```bash
cargo run
```

Bring up the CAN interface at 125kbit/s on your host computer:
```bash
sudo ip link set can0 up type can bitrate 125000
```

Use the zencan CLI to scan for the ESP node:
```bash
cargo run --bin zencan-cli can0

can0>scan                                                                                  Nodes: 1
Node 24: PreOperational
    Identity vendor: CAFE, product: 408, revision: 1, serial: FA186B18
    Device Name: 'esp-node'
    Versions: '001' SW, '1' HW
    Last Seen: 0s ago
```
