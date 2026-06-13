//! Integration tests that load USB descriptor fixtures from
//! `tests/fixtures/` and verify they parse correctly against their
//! TOML sidecar metadata.
//!
//! Each subdirectory of `tests/fixtures/` must contain:
//!   - `descriptor.bin`  — raw USB descriptor tree bytes
//!   - `metadata.toml`   — expected property assertions

use std::fs;
use std::path::Path;

use serde::Deserialize;
use usbip_core::descriptor::{EndpointType, UsbDeviceInfo};

/// Top-level TOML sidecar structure.
/// Every field is optional: `None` means "do not assert."
#[derive(Debug, Deserialize)]
struct FixtureMetadata {
    device: Option<DeviceMeta>,
    config: Option<ConfigMeta>,
    interface: Option<InterfaceMeta>,
    endpoints: Option<Vec<EndpointMeta>>,
}

#[derive(Debug, Deserialize)]
struct DeviceMeta {
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    device_class: Option<u8>,
    device_sub_class: Option<u8>,
    device_protocol: Option<u8>,
    max_packet_size0: Option<u8>,
    num_configurations: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct ConfigMeta {
    num_configs: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct InterfaceMeta {
    class: Option<u8>,
    sub_class: Option<u8>,
    protocol: Option<u8>,
    num_endpoints: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct EndpointMeta {
    address: Option<u8>,
    transfer_type: Option<String>,
}

/// Discover all fixture directories under the given root.
fn discover_fixtures(root: &Path) -> Vec<std::path::PathBuf> {
    let mut fixtures = Vec::new();
    for entry in fs::read_dir(root).expect("fixtures directory must exist") {
        let entry = entry.expect("readable entry");
        let path = entry.path();
        if path.is_dir() {
            // Must contain both descriptor.bin and metadata.toml
            if path.join("descriptor.bin").exists() && path.join("metadata.toml").exists() {
                fixtures.push(path);
            }
        }
    }
    fixtures.sort();
    fixtures
}

/// Load and verify a single fixture.
fn verify_fixture(fixture_path: &Path) {
    let name = fixture_path.file_name().unwrap().to_str().unwrap();
    let bin_path = fixture_path.join("descriptor.bin");
    let meta_path = fixture_path.join("metadata.toml");

    let raw = fs::read(&bin_path).unwrap_or_else(|e| {
        panic!("fixture '{name}': failed to read descriptor.bin: {e}");
    });

    let meta_toml = fs::read_to_string(&meta_path).unwrap_or_else(|e| {
        panic!("fixture '{name}': failed to read metadata.toml: {e}");
    });

    let metadata: FixtureMetadata = toml::from_str(&meta_toml).unwrap_or_else(|e| {
        panic!("fixture '{name}': failed to parse metadata.toml: {e}");
    });

    let info = UsbDeviceInfo::parse_descriptor_tree(&raw).unwrap_or_else(|| {
        panic!("fixture '{name}': parse_descriptor_tree returned None");
    });

    // Assert device-level fields
    if let Some(ref dev) = metadata.device {
        if let Some(vid) = dev.vendor_id {
            assert_eq!(info.device.id_vendor, vid, "fixture '{name}': device.id_vendor mismatch");
        }
        if let Some(pid) = dev.product_id {
            assert_eq!(info.device.id_product, pid, "fixture '{name}': device.id_product mismatch");
        }
        if let Some(cls) = dev.device_class {
            assert_eq!(
                info.device.b_device_class, cls,
                "fixture '{name}': device.b_device_class mismatch"
            );
        }
        if let Some(sub) = dev.device_sub_class {
            assert_eq!(
                info.device.b_device_sub_class, sub,
                "fixture '{name}': device.b_device_sub_class mismatch"
            );
        }
        if let Some(proto) = dev.device_protocol {
            assert_eq!(
                info.device.b_device_protocol, proto,
                "fixture '{name}': device.b_device_protocol mismatch"
            );
        }
        if let Some(mps) = dev.max_packet_size0 {
            assert_eq!(
                info.device.b_max_packet_size0, mps,
                "fixture '{name}': device.b_max_packet_size0 mismatch"
            );
        }
        if let Some(num) = dev.num_configurations {
            assert_eq!(
                info.device.b_num_configurations, num,
                "fixture '{name}': device.b_num_configurations mismatch"
            );
        }
    }

    // Assert config-level fields
    if let Some(ref cfg) = metadata.config {
        if let Some(count) = cfg.num_configs {
            assert_eq!(info.configs.len(), count, "fixture '{name}': config count mismatch");
        }
    }

    // Assert the first interface (if present)
    if let Some(ref iface) = metadata.interface {
        if let Some(info_iface) = info.configs.first().and_then(|c| c.interfaces.first()) {
            if let Some(cls) = iface.class {
                assert_eq!(
                    info_iface.interface.b_interface_class, cls,
                    "fixture '{name}': interface.class mismatch"
                );
            }
            if let Some(sub) = iface.sub_class {
                assert_eq!(
                    info_iface.interface.b_interface_sub_class, sub,
                    "fixture '{name}': interface.sub_class mismatch"
                );
            }
            if let Some(proto) = iface.protocol {
                assert_eq!(
                    info_iface.interface.b_interface_protocol, proto,
                    "fixture '{name}': interface.protocol mismatch"
                );
            }
            if let Some(num) = iface.num_endpoints {
                assert_eq!(
                    info_iface.endpoints.len() as u8, num,
                    "fixture '{name}': interface endpoint count mismatch (expected={num}, actual={})",
                    info_iface.endpoints.len()
                );
            }
        } else {
            panic!("fixture '{name}': no interface found but metadata declares interface fields");
        }
    }

    // Assert endpoint-level fields
    if let Some(ref endpoints_meta) = metadata.endpoints {
        let info_iface = info
            .configs
            .first()
            .and_then(|c| c.interfaces.first())
            .expect("fixture '{name}': no interface for endpoint assertions");

        assert_eq!(
            info_iface.endpoints.len(),
            endpoints_meta.len(),
            "fixture '{name}': endpoint count mismatch (expected={}, actual={})",
            endpoints_meta.len(),
            info_iface.endpoints.len()
        );

        for (i, ep_meta) in endpoints_meta.iter().enumerate() {
            let ep = &info_iface.endpoints[i];
            if let Some(addr) = ep_meta.address {
                assert_eq!(
                    ep.b_endpoint_address, addr,
                    "fixture '{name}': endpoint[{i}].address mismatch"
                );
            }
            if let Some(ref tt) = ep_meta.transfer_type {
                let expected = parse_transfer_type(tt);
                assert_eq!(
                    ep.transfer_type(),
                    expected,
                    "fixture '{name}': endpoint[{i}].transfer_type mismatch"
                );
            }
        }
    }
}

fn parse_transfer_type(s: &str) -> EndpointType {
    match s {
        "control" => EndpointType::Control,
        "isochronous" => EndpointType::Isochronous,
        "bulk" => EndpointType::Bulk,
        "interrupt" => EndpointType::Interrupt,
        other => {
            panic!("unknown transfer_type '{other}' (expected control|isochronous|bulk|interrupt)")
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn all_fixtures_parse_correctly() {
    let fixtures_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let fixtures = discover_fixtures(&fixtures_root);

    // Exclude suite-specific test fixtures (e.g. *_mismatch)
    let regular_fixtures: Vec<_> = fixtures
        .iter()
        .filter(|p| {
            let name = p.file_name().unwrap().to_str().unwrap();
            !name.ends_with("_mismatch")
        })
        .collect();

    assert!(!regular_fixtures.is_empty(), "no fixture directories found in {fixtures_root:?}");

    for fixture in &regular_fixtures {
        verify_fixture(fixture);
    }
}

#[test]
#[should_panic(expected = "device.b_device_class mismatch")]
fn fixture_mismatch_produces_clear_error() {
    let fixtures_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mismatch = fixtures_root.join("g920_mismatch");
    verify_fixture(&mismatch);
}
