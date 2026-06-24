use integration_tests::object_dict1::get_od;
use zencan_node::object_dict::ObjectAccess;

#[test]
fn test_event_flags() {
    let od = get_od();
    fn test_event_flags(obj: &dyn ObjectAccess, n: u8) {
        let od = get_od();
        // No flags set after toggle
        od.node_state().object_flag_sync().toggle();
        for i in 0..n {
            assert!(!obj.read_event_flag(i));
        }
        // Set all the flags on the object
        for i in 0..n {
            obj.set_event_flag(i).unwrap();
        }

        // Toggle and read back set flags
        od.node_state().object_flag_sync().toggle();
        for i in 0..n {
            assert!(obj.read_event_flag(i));
        }

        // Set only the first flag
        obj.set_event_flag(0).unwrap();

        // Toggle and check they are cleared, except the first
        od.node_state().object_flag_sync().toggle();

        for i in 0..n {
            assert_eq!(i == 0, obj.read_event_flag(i));
        }
    }

    test_event_flags(od.object3008(), 7);
    test_event_flags(od.object3009(), 8);
    test_event_flags(od.object300a(), 9);
}
