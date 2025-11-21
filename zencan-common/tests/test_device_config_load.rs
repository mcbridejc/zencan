use zencan_common::{
    device_config::{DeviceConfig, PdoDefaultConfig},
    pdo::PdoMapping,
};

/// Exercise loading defaults for zencan defined objects
#[test]
fn test_system_defaults() {
    const DEVCFG: &str = r#"
        device_name = "test"

        [identity]
        vendor_id = 1234
        product_code = 12000
        revision_number = 1

        [pdos]
        # Setup default map for TPDO 0
        [pdos.tpdo.0]
        enabled = true
        cob_id = 0x200
        add_node_id = true
        transmission_type = 254
        mappings = [
            { index=0x2000, sub=0, size=16 },
        ]

        [pdos.rpdo.2]
        enabled = false
        cob_id = 0x201
        add_node_id = false
        transmission_type = 0
        mappings = [
            { index=0x2001, sub=1, size=8 },
            { index=0x2001, sub=2, size=8 },
        ]
    "#;

    let cfg = DeviceConfig::load_from_str(DEVCFG).expect("Failed to parse device config");

    assert_eq!(
        *cfg.pdos.tpdo_defaults.get(&0).unwrap(),
        PdoDefaultConfig {
            cob_id: 0x200,
            extended: false,
            add_node_id: true,
            enabled: true,
            rtr_disabled: false,
            mappings: vec![PdoMapping {
                index: 0x2000,
                sub: 0,
                size: 16
            }],
            transmission_type: 254
        }
    );

    assert_eq!(
        *cfg.pdos.rpdo_defaults.get(&2).unwrap(),
        PdoDefaultConfig {
            cob_id: 0x201,
            extended: false,
            enabled: false,
            add_node_id: false,
            rtr_disabled: false,
            mappings: vec![
                PdoMapping {
                    index: 0x2001,
                    sub: 1,
                    size: 8
                },
                PdoMapping {
                    index: 0x2001,
                    sub: 2,
                    size: 8
                }
            ],
            transmission_type: 0
        }
    );
}
