//! Tests that only validate the generated code
//!

use integration_tests::{object_dict1, object_dict2, object_dict3};
use zencan_node::object_dict::find_object;

#[test]
fn test_autostart_defaults() {
    // Example 1 should default to disabled
    assert_eq!(0, object_dict1::OBJECT5000.get_value());
    // Example 2 should default to enabled
    assert_eq!(1, object_dict2::OBJECT5000.get_value());
    // Example 3 should have no object 5000
    assert!(find_object(&object_dict3::OD_TABLE, 0x5000).is_none())
}
