# CAN FD

Zencan nodes can run on CAN FD: frames of up to 64 data bytes, with an optional bit rate switch
(BRS) for the data phase. A node selects CAN FD in its device config; nodes that stay on classic
CAN pay nothing for it, in RAM or in flash.

## Model

### Two frame types

`zencan_common::can` has two concrete frame types:

| Type           | Payload    | Remote frames | Extra fields                                |
| -------------- | ---------- | ------------- | ------------------------------------------- |
| `CanMessage`   | 0–8 bytes  | yes           | –                                           |
| `CanFdMessage` | 0–64 bytes | no            | `brs` (bit rate switch), `esi` (error state) |

`CanMessage` is unchanged: same fields, same layout, same inherent API, including the panicking
`CanMessage::new`. Both types implement the `CanFrame` trait, which gives generic code uniform
access: `try_new`, `try_new_zeroed`, `try_new_rtr`, `id`, `data`, `data_mut`, `len`, `is_rtr`,
`on_wire_len`, `padded_data`, and the `MAX_LEN`, `SUPPORTS_RTR` and `IS_FD` constants.

Conversions use `TryFrom` in both directions. Widening a classic frame fails for remote frames, and
narrowing a CAN FD frame fails above 8 bytes. There is no `From<CanMessage> for CanFdMessage`,
because a classic RTR frame has no CAN FD equivalent.

### What changes on CAN FD, and what does not

Only PDOs grow: a CAN FD PDO carries up to 64 bytes. Every other protocol zencan implements (SDO,
NMT, LSS, heartbeat, SYNC) fits in 8 bytes and keeps exactly its CiA 301 encoding. These messages are
still built as `CanMessage` and are converted to the node's frame type at the mailbox boundary. On a
CAN FD bus they travel in 8-byte CAN FD frames, with the data phase at the data bit rate.

The node's transport path is generic over the frame type, with `CanMessage` as the default
everywhere: `NodeMbox<F>`, `Node<F>`, `NodeState<F>`, `Pdo<F>`, `PdoData<F>`, the PDO objects,
and the transport traits `CanSender<F>`, `CanReceiver<F>`, `AsyncCanSender<F>` and
`AsyncCanReceiver<F>`. Code written before CAN FD support compiles unchanged.

### Why classic nodes pay nothing

- **The two frame types are separate.** A type which is never instantiated costs no RAM and no
  flash, so a classic node carries no 64-byte buffers.
- **The switch is in the device config, not a Cargo feature.** Cargo features are unified across
  the dependency graph: one CAN FD node in a workspace would turn every node into a 64-byte one.
  The device config is per node, and already drives code generation.
- **Frame-dependent code is compiled inside zencan-node.** Node types are bound by the sealed
  `NodeFrame` trait, implemented for `CanMessage` and `CanFdMessage`. Its impls route the large
  entry points (`Node::new`/`process`, `NodeMbox::store_message`/`next_transmit_message`,
  `PdoData::default`, `Pdo::init_defaults`) to code monomorphized in zencan-node. The application
  therefore links precompiled code, as it did before, and does not instantiate the node logic
  itself. With plain generics, LLVM inlined `Node::process` into `main` and stm32l5-minimal grew by
  about 880 bytes.
- **PDO buffers hold only the payload.** `PdoData::buffered_value` is an
  `Option<F::PdoPayload>`, i.e. a `PdoBuffer<8>` for classic and a `PdoBuffer<64>` for CAN FD, not a
  whole frame. The frame would repeat the COB-ID the PDO already stores.

Example binary sizes (bytes, `scripts/example_sizes.sh`), before CAN FD support and after:

| Example              | Text (before → after) | Data         | BSS          | RAM (data + bss) |
| -------------------- | --------------------- | ------------ | ------------ | ---------------- |
| stm32g0-lilos-node   | 58212 → 57772         | 132 → 132    | 3180 → 3148  | 3312 → 3280      |
| stm32l5-minimal      | 31790 → 31698         | 132 → 132    | 2488 → 2456  | 2620 → 2588      |
| esp-node             | 475474 → 475456       | 1824 → 1816  | 4172 → 4140  | 5996 → 5956      |
| stm32l5-minimal (fd) | 34550 (new)           | 356          | 2984         | 3340             |

Classic builds got slightly smaller. TPDOs are now read straight into a zero-initialized buffer,
which removes a stack array, a copy and a panic path. `Option<PdoBuffer<8>>` is 10 bytes, against
16 for the `Option<heapless::Vec<u8, 8>>` it replaces; after alignment, each PDO is 4 bytes smaller
(the examples have 8 PDOs each, hence 32 bytes less BSS).

## Building a CAN FD node

### 1. Select CAN FD in the device config

```toml
[can]
frame_format = "fd"   # "classic" is the default when the section is absent
```

PDOs may then map up to 64 bytes, still in at most 8 byte-aligned entries. CAN FD has no remote
frames, so every default PDO must set `rtr_disabled = true`:

```toml
[pdos.tpdo.0]
enabled = true
cob_id = 0x180
add_node_id = true
rtr_disabled = true      # required with frame_format = "fd"
transmission_type = 254
mappings = [
    { index = 0x2000, sub = 1, size = 64 },
    { index = 0x2000, sub = 2, size = 64 },
    # ... up to 8 entries, 64 bytes in total
]
```

zencan-build rejects PDO defaults over 8 bytes in classic mode ("mappings exceed one CAN frame") or
over 64 bytes in CAN FD mode, and rejects CAN FD PDO defaults that keep RTR enabled.

### 2. Use the generated frame type in the application

The generated module exports the frame type it was built for, as `Frame`:

```rust
mod zencan {
    zencan_node::include_modules!(ZENCAN_CONFIG);
}
use zencan::Frame; // CanFdMessage when frame_format = "fd", CanMessage otherwise
```

It also contains a compile-time assertion that `Frame` matches the configured format. The
mailbox, node state and node are all typed by it. `Node::new(...)` infers the frame type from the
mailbox, so application code that creates and runs the node is unchanged.

### 3. Adapt the CAN driver

The driver is the only code that has to know about CAN FD. For a classic driver moving to CAN FD,
the minimal diff is:

```diff
-let message = CanMessage::new(id, &buffer[..len]);
+let message = Frame::new(id, &buffer[..len]);      // a 64-byte receive buffer
 node_mbox.store_message(message).ok();

 while let Some(msg) = node_mbox.next_transmit_message() {
-    transmit(msg.id(), msg.data(), /* classic */);
+    // Send on_wire_len() bytes: the payload padded with zeros to a legal CAN FD length
+    transmit(msg.id(), msg.padded_data(), /* FD frame, BRS = msg.brs() */);
 }
```

And configure the controller for CAN FD with a data bit timing. Written against `Frame`, the same
driver serves both formats; see `examples/stm32l5-minimal/src/can.rs`, which builds for CAN FD with
`--features fd`. Its classic build is byte-identical to the one before this support.

### On Linux (socketcan)

```rust
let (tx, rx) = zencan_node::open_socketcan_fd("can0")?;
```

`open_socketcan_fd` opens the interface with `CAN_RAW_FD_FRAMES`. Before opening, it checks over
netlink that the interface MTU is 72, the CAN FD MTU (configure the interface with
`ip link set can0 type can bitrate 500000 dbitrate 2000000 fd on`, or
`ip link set vcan0 mtu 72` for a vcan device). Transmitted frames are padded and carry the frame's
BRS flag. Received CAN FD frames keep their BRS and ESI flags, classic data frames are widened to
`CanFdMessage`, and remote frames are discarded.

To use the classic-frame client code (`SdoClient`, `NmtMaster`, `LssMaster`, `BusManager`) on a CAN
FD bus, wrap the handles in `ClassicAdapter`. It sends classic frames as CAN FD frames and skips
received frames longer than 8 bytes:

```rust
use zencan_client::common::can::ClassicAdapter;
let (tx, rx) = zencan_client::open_socketcan_fd("can0")?;
let manager = BusManager::new(ClassicAdapter::new(tx), ClassicAdapter::new(rx));
```

`zencan-cli --fd can0` and `zencandump --fd can0` do exactly this.

## Frame details

- **Padding.** CAN FD payloads are 0–8, 12, 16, 20, 24, 32, 48 or 64 bytes long (`fd_padded_len`).
  `len()` is the payload length and `on_wire_len()` the transmitted length. `padded_data()` is the
  payload followed by zeros, since bytes past `len()` in a `CanFdMessage` are always zero. Padding
  is the driver's job. A receiver sees the padded length; RPDO reception uses the mapping lengths, so
  the padding never reaches an object. The SDO mailbox and `BlockSegment` parsing accept frames
  longer than 8 bytes and use the first 8.
- **Bit rate switching.** `CanFdMessage::new`, `try_new` and the classic-to-FD conversion request
  BRS. The controller decides whether the data phase actually switches. Classic frames received on
  a CAN FD socket are widened without BRS.
- **Error state indicator.** ESI is reported on reception. On transmission the controller sets it
  from its own error state.
- **Remote frames.** `CanFdMessage::try_new_rtr` fails. A CAN FD PDO always reports and keeps the
  RTR-disabled bit (bit 30 of its COB-ID), whatever its defaults or an SDO write request.
  Requesting a TPDO by RTR is not available.

## Limits

- **8 mapping entries per PDO** (`N_MAPPING_PARAMS`), byte-aligned only. With entries of up to 8
  bytes, this fills a 64-byte frame. Raising the count would cost RAM on every PDO.
- **SDO stays CiA 301**, in 7-byte segments. A CAN FD node's SDO server answers in 8-byte CAN FD
  frames, so a client must listen on a CAN FD capable socket (`open_socketcan_fd`).
- **Bus compatibility.** CAN FD frames are protocol errors for controllers that only know classic
  CAN. Every node on the bus must be CAN FD capable, or at least CAN FD tolerant.
- **PDO mappings written over SDO at runtime** are not checked against the frame size. As before
  CAN FD, mappings beyond the frame capacity are just not transmitted.
- **stm32l5-minimal runs its data phase at 1 Mbit/s.** Faster rates need transceiver delay
  compensation, which fdcan 0.2 does not configure.

## Design decisions and deviations from the initial design

The initial design called for two frame types, a `CanFrame` access trait, and node types generic
over the frame with `CanMessage` as the default. That is what was built, with the following
deviations:

1. **Transport traits take a defaulted type parameter, not an associated `Frame` type.**
   Associated type defaults are unstable (rust-lang/rust#29661). Without them, every existing
   implementor would have to add `type Frame = CanMessage;`, including test mocks such as the
   `MockReceiver` in zencan-client. With a defaulted parameter, `impl AsyncCanReceiver for X` still
   means classic CAN and existing code compiles unchanged. The parameter allows one type to
   implement both frame types, but no zencan type does: `ClassicAdapter` is a separate wrapper.
2. **A sealed `NodeFrame` trait bounds the node types**, rather than
   `F: CanFrame + TryFrom<CanMessage>` and `CanMessage: TryFrom<F>`. Beyond being one bound, it moves
   monomorphization of the node logic into zencan-node, which keeps classic binaries from growing
   (see above). The set of node frame types is therefore closed to `CanMessage` and `CanFdMessage`,
   which are the two CAN formats.
3. **PDO buffers store the payload, not the frame.** `buffered_value` was meant to become
   `AtomicCell<Option<F>>`. `Option<CanMessage>` is 20 bytes on 32-bit targets, against 16 before,
   which would have cost classic nodes 4 bytes of RAM per PDO. An associated type of `NodeFrame`,
   `PdoBuffer<8>` or `PdoBuffer<64>`, avoids `generic_const_exprs` just as well, and saves 4 bytes
   per PDO against the old buffer. `send_pdo` reads mapped objects straight into it, with no
   separate stack array. `take_frame` builds the frame from it with `F::try_new`.
4. **`FrameError` instead of `FrameLenError`.** The error also reports `RtrNotSupported`, which is
   not a length error.
5. **`CanFrame::padded_data()` was added** to the trait: drivers need the zero-padded on-wire bytes,
   and generic driver code cannot reach `CanFdMessage` internals.
6. **`PdoData::new()` builds classic storage only**, like `HashMap::new`. `PdoData::default()` is
   generic. An unconstrained `PdoData::new()` in existing code could not otherwise infer its frame
   type.
7. **CAN FD PDO defaults need an explicit `rtr_disabled = true`.** The field defaults to `false`
   when absent, and keeping it a `bool` keeps `PdoDefaultConfig` source compatible.
8. **Sizes are smaller, not identical.** Making the node types generic changes how LLVM inlines code
   even with the `NodeFrame` firewall. On its own, the generic transport path moved the examples by
   −44 to +54 bytes of text, with identical RAM. With the PDO buffer change, every example is
   smaller in every section than before.
9. **Additions beyond the plan:** `ClassicAdapter`, the `--fd` flag of zencan-cli and zencandump,
   and padding tolerance in the SDO mailbox (`sdo_comms.rs`). These are needed to drive a CAN FD
   node from Linux.

## Possible follow-ups

- **USDO (CiA 1301).** CANopen FD replaces the CiA 301 SDO with the universal SDO: multicast and
  broadcast addressing, and up to 56 data bytes per frame. That is a different protocol, with its
  own COB-IDs and state machines. It belongs beside `zencan-node/src/sdo_server/`, as a new server
  (and a new client), not as a modification of the CiA 301 one, which CAN FD nodes keep serving.
- **Configurable mapping count.** `N_MAPPING_PARAMS` could become a const generic parameter
  (default 8) driven by the device config, for PDOs of many small objects.
- **Sub-byte PDO mapping.** Still unsupported (`pdo.rs` only accepts multiples of 8 bits).
- **SRDO, LSS on CAN FD, DBC generation for CAN FD.**
- **Transceiver delay compensation** in the fdcan-based examples, for data phases above 1 Mbit/s.
- **Runtime PDO mapping validation**: reject a mapping count whose total exceeds the frame size
  (abort code 0x06040042), for both formats.
