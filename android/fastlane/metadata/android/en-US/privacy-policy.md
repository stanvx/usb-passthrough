# Privacy Policy

**Last updated: June 2026**

## Overview

AnyPlug ("the App") is a USB/IP protocol passthrough bridge that enables exporting and importing USB devices over a network. This privacy policy explains how the App handles data.

## Data Collection

AnyPlug does **not** collect, store, or transmit any personal data, usage analytics, or telemetry. The App operates entirely on your local network.

## Network Communication

- **Local Network Only**: AnyPlug discovers USB/IP servers and transfers USB data exclusively over your local area network using the USB/IP protocol (TCP port 3240 by default).
- **No External Servers**: The App does not connect to any external servers, cloud services, or third-party endpoints.
- **Encryption**: When enabled, encryption (AES-256-GCM) is applied to local network traffic only and does not involve any external key servers.

## USB Device Data

- USB/IP protocol data (URB requests and responses) is forwarded between devices on your local network.
- AnyPlug does not inspect, log, or store the content of USB transfers.
- USB device identifiers (vendor ID, product ID, device name) are used only for device selection and are not transmitted outside your local network.

## Permissions

- **USB Host API**: Required to access USB devices attached to the device.
- **Internet**: Required for TCP socket communication on the local network only.
- **Foreground Service**: Required to maintain USB/IP connections in the background.

## Third-Party Services

This App uses no third-party analytics, advertising, or tracking services.

## Changes to This Policy

Updates will be documented in the App's source repository. Continued use of the App after changes constitutes acceptance of the updated policy.

## Contact

For questions about this privacy policy, please open an issue in the project repository.

---

**Open Source**: AnyPlug is open source software. The complete source code is available for review.
