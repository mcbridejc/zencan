//! Tests that only validate the generated code
//!

use integration_tests::{object_dict1, object_dict2, object_dict3};
use zencan_node::object_dict::find_object;

#[test]
fn test_autostart_defaults() {
    // Example 1 should default to disabled
    assert_eq!(0, object_dict1::get_od().object5000().get_value());
    // Example 2 should default to enabled
    assert_eq!(1, object_dict2::get_od().object5000().get_value());
    // Example 3 should have no object 5000
    assert!(find_object(object_dict3::get_od().od_table(), 0x5000).is_none())
}

#[test]
fn lookup_entries_reference_generated_objects() {
    let od1 = object_dict1::get_od();
    let od2 = object_dict2::get_od();
    let od3 = object_dict3::get_od();
    assert!(!od1.od_table().is_empty());
    assert!(!od2.od_table().is_empty());
    assert!(!od3.od_table().is_empty());
    let mapped = find_object(od1.od_table(), 0x2000).unwrap();
    assert!(core::ptr::eq(
        mapped as *const dyn zencan_node::object_dict::ObjectAccess as *const (),
        od1.object2000() as *const _ as *const (),
    ));
}
