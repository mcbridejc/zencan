use integration_tests::object_dict1::{NODE_STATE, OBJECT3008, OBJECT3009, OBJECT300A};
use zencan_node::object_dict::ObjectAccess;

#[test]
fn test_event_flags() {
    fn test_event_flags(obj: &dyn ObjectAccess, n: u8) {
        // No flags set after toggle
        NODE_STATE.pdo_sync().toggle();
        for i in 0..n {
            assert!(!obj.read_event_flag(i));
        }
        // Set all the flags on the object
        for i in 0..n {
            obj.set_event_flag(i).unwrap();
        }

        // Toggle and read back set flags
        NODE_STATE.pdo_sync().toggle();
        for i in 0..n {
            assert!(obj.read_event_flag(i));
        }

        // Set only the first flag
        obj.set_event_flag(0).unwrap();

        // Toggle and check they are cleared, except the first
        NODE_STATE.pdo_sync().toggle();

        for i in 0..n {
            assert_eq!(i == 0, obj.read_event_flag(i));
        }
    }

    test_event_flags(&OBJECT3008, 7);
    test_event_flags(&OBJECT3009, 8);
    test_event_flags(&OBJECT300A, 9);
}
