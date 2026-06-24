use zencan_node::object_dict::ObjectAccess as _;

zencan_node::build_object_dict!(
    r#"
    device_name = "macro_test"

    [identity]
    vendor_id = 1
    product_code = 2
    revision_number = 3

    [[objects]]
    index = 0x2000
    parameter_name = "Speed"
    data_type = "UInt16"
    access_type = "rw"
    object_type = "var"
    default_value = 42
    "#
);

#[test]
fn inline_dictionary_initializes_defaults() {
    let od = get_od();
    assert_eq!(od.object2000().read_u16(0), Ok(42));
    assert!(core::ptr::eq(od, get_od()));
}
