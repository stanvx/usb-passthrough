# USB/IP Protocol

Reference for this project's wire format. Based on the Linux kernel USB/IP protocol
with encryption and compression extensions.

---

## 1. Transport

```
TCP, port 3240 (IANA registered) — see [PORTS.md](docs/api/PORTS.md) for the full port model.
All multi-byte integers: BIG-ENDIAN (network byte order)
Exception: encapsulated USB payload is LITTLE-ENDIAN (USB native)

Connection flow:
  Server: LISTEN → ACCEPT
  Client: CONNECT → send OP_REQ_DEVLIST → receive OP_REP_DEVLIST
  Client: send OP_REQ_IMPORT → receive OP_REP_IMPORT
  Client: URB exchange begins
```

## 2. Common Header

Every USB/IP message begins with an 8-byte header:

```
Offset  Size  Field           Description
──────────────────────────────────────────────────
0x00    2     version         0x0111 (USB/IP v1.1.1)
0x02    2     command         0x8003 = OP_REQ_DEVLIST
                              0x0005 = OP_REP_DEVLIST
                              0x8006 = OP_REQ_IMPORT
                              0x0007 = OP_REP_IMPORT
                              0x0001 = USBIP_CMD_SUBMIT (URB)
                              0x0003 = USBIP_RET_SUBMIT (URB reply)
                              0x0002 = USBIP_RET_UNLINK
0x04    4     status          For replies: 0 = success
```

```rust
#[repr(C, packed)]
pub struct UsbIpHeader {
    pub version: u16be,
    pub command: u16be,
    pub status:  i32be,
}
```

## 3. Device Enumeration

### 3.1 OP_REQ_DEVLIST (command = 0x8003)

```
Client → Server

Payload: empty (just the 8-byte header)
```

### 3.2 OP_REP_DEVLIST (command = 0x0005)

```
Server → Client

Payload:
  Offset  Size  Field
  ─────────────────────────────────
  0x00    4     ndev (number of devices)
  0x04    n*... device entries (see below)

Device entry (312 bytes each):
  Offset  Size  Field
  ───────────────────────────────────
  0x00    256   path    (sysfs device path, UTF-8, null-padded)
  0x100   32    busid   (USB bus ID, e.g., "1-1.2", null-padded)
  0x120   4     busnum
  0x124   4     devnum
  0x128   4     speed   (1=low, 2=full, 3=high, 4=super, 5=super+)
  0x12C   2     idVendor
  0x12E   2     idProduct
  0x130   2     bcdDevice
  0x132   1     bDeviceClass
  0x133   1     bDeviceSubClass
  0x134   1     bDeviceProtocol
  0x135   1     bConfigurationValue
  0x136   1     bNumConfigurations
  0x137   1     bNumInterfaces
```

```rust
#[repr(C, packed)]
pub struct UsbIpDeviceEntry {
    pub path:                [u8; 256],
    pub busid:               [u8; 32],
    pub busnum:              u32be,
    pub devnum:              u32be,
    pub speed:               u32be,
    pub id_vendor:           u16be,
    pub id_product:          u16be,
    pub bcd_device:          u16be,
    pub b_device_class:      u8,
    pub b_device_sub_class:  u8,
    pub b_device_protocol:   u8,
    pub b_configuration_value: u8,
    pub b_num_configurations:  u8,
    pub b_num_interfaces:    u8,
}
```

## 4. Device Import

### 4.1 OP_REQ_IMPORT (command = 0x8006)

```
Client → Server

Payload:
  Offset  Size  Field
  ──────────────────────
  0x00    32    busid (null-padded UTF-8)
```

### 4.2 OP_REP_IMPORT (command = 0x0007)

```
Server → Client

Success (status = 0):
  Offset  Size  Field
  ───────────────────────────────────
  0x00    312   device entry (same format as OP_REP_DEVLIST)
  (followed by full device descriptor)

Failure (status != 0):
  Just the header with error status.
```

After OP_REP_IMPORT success, server sends the full USB device descriptor tree:
- Device descriptor (18 bytes)
- Each configuration descriptor with all interface/endpoint descriptors

```
Payload continues after device entry:

  Offset  Size  Field
  ──────────────────────────
  0x00    18    device_descriptor
  0x12    9     config_descriptor
  0x1B    9     interface_descriptor
  0x24    7     endpoint_descriptor (IN)
  0x2B    7     endpoint_descriptor (OUT)
  ... more descriptors as needed
```

## 5. URB Exchange (Data Transfer)

### 5.1 USBIP_CMD_SUBMIT (command = 0x0001)

```
Client → Server

  Offset  Size  Field
  ─────────────────────────────────────────
  0x00    4     seqnum         (monotonically increasing)
  0x04    4     devid          (assigned by client)
  0x08    4     direction      (0 = OUT host→device, 1 = IN device→host)
  0x0C    4     ep             (endpoint number, direction in bit 7)
  0x10    4     transfer_flags (USB/IP flags)
  0x14    4     transfer_buffer_length
  0x18    4     start_frame    (isochronous only)
  0x1C    4     number_of_packets (isochronous only)
  0x20    4     interval       (interrupt only)
  0x24    8     setup          (control transfer setup packet, 8 bytes)
  ─────────────────────────────────────────
  [if direction == OUT:]
  0x2C    N     data           (actual USB data payload)
```

**Transfer flags (bitfield):**
```
Bit 0: URB_SHORT_NOT_OK     (report short reads as errors)
Bit 1: URB_ISO_ASAP         (schedule isochronous ASAP)
Bit 2: URB_NO_TRANSFER_DMA_MAP
Bit 3: URB_ZERO_PACKET      (send zero-length packet at end of bulk OUT)
Bit 4: URB_NO_INTERRUPT     (don't interrupt on completion)
Bit 5: URB_FREE_BUFFER
Bit 6: URB_DIR_IN           (data direction: 0=OUT, 1=IN)
Bit 7: URB_DIR_OUT
```

```rust
#[repr(C, packed)]
pub struct UsbIpCmdSubmit {
    pub seqnum:                 u32be,
    pub devid:                  u32be,
    pub direction:              u32be,
    pub ep:                     u32be,
    pub transfer_flags:         u32be,
    pub transfer_buffer_length: u32be,
    pub start_frame:            u32be,
    pub number_of_packets:      u32be,
    pub interval:               u32be,
    pub setup:                  [u8; 8],
    // variable-length data follows
}
```

### 5.2 USBIP_RET_SUBMIT (command = 0x0003)

```
Server → Client

  Offset  Size  Field
  ─────────────────────────────────────────
  0x00    4     seqnum         (echoes CMD_SUBMIT seqnum)
  0x04    4     devid
  0x08    4     direction
  0x0C    4     ep
  0x10    4     status         (URB completion status, 0 = success)
  0x14    4     actual_length  (bytes transferred, or error code)
  0x18    4     start_frame    (isochronous only)
  0x1C    4     number_of_packets
  0x20    4     error_count
  0x24    8     setup          (echoed)
  ─────────────────────────────────────────
  [if direction == IN and status == 0:]
  0x2C    N     data           (received USB data payload)
```

### 5.3 USBIP_RET_UNLINK (command = 0x0002)

```
Server → Client (asynchronous — server cancels an in-flight URB)

  Offset  Size  Field
  ─────────────────────
  0x00    4     seqnum
  0x04    4     devid
  0x08    4     status
```

## 6. Encryption Extension (non-standard)

When encryption is enabled, after the initial TCP handshake:

```
Client                                    Server
  │                                         │
  │  ECDH key exchange (X25519)             │
  │────────────────────────────────────────►│
  │                                         │
  │                  ECDH response           │
  │◄────────────────────────────────────────│
  │                                         │
  │  HKDF-SHA256 → AES-256-GCM session key │
  │                                         │
  │  All subsequent USB/IP messages:        │
  │  [4-byte ciphertext length][ciphertext] │
  │  Each message has unique 96-bit nonce   │
  │  (initialized from session key + seq)   │
  │                                         │
```

## 7. Compression Extension (optional)

For bulk endpoints on slow links:

```
After encryption (if enabled):

  [1 byte: algorithm] [4 bytes: uncompressed_len] [compressed payload]

  0x00 = no compression
  0x01 = LZ4
  0x02 = Zstandard

  Compression is per-URB, not per-stream.
  Only applied when compressed_size < uncompressed_size - threshold (64 bytes).
```

## 8. Error Codes

```
USBIP status codes (in OP_REP_* status field):
  0x00000000  SUCCESS
  0x00000001  ST_NA       (device not available)
  0x00000002  ST_DEV_BUSY (device already exported to another client)
  0x00000003  ST_DEV_ERR  (device error)
  0x00000004  ST_NODEV    (no such device)
  0x00000005  ST_ERROR    (generic error)

URB status codes (in USBIP_RET_SUBMIT.status):
  Standard USB error codes (from linux/usb.h):
  0       -EPROTO    (protocol error)
  -2      -ENOENT    (no such file)
  -5      -EIO       (I/O error)
  -11     -EAGAIN    (try again)
  -18     -EXDEV     (cross-device link)
  -19     -ENODEV    (no such device)
  -22     -EINVAL    (invalid argument)
  -32     -EPIPE     (broken pipe / STALL)
  -62     -ETIME     (timer expired)
  -63     -ENOSR     (out of streams resources)
  -71     -EPROTO    (protocol error)
  -75     -EOVERFLOW (value too large)
  -84     -EILSEQ    (illegal byte sequence)
  -108    -ESHUTDOWN (cannot send after transport endpoint shutdown)
```
