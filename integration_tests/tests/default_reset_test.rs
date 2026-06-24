//! Reset semantics and the generated initialization boundary.
use integration_tests::object_dict1::get_od;
use serial_test::serial;
use zencan_node::{
    object_dict::{ObjectAccess, ObjectDictionary},
    ResetScope,
};

#[test]
fn concurrent_first_access_publishes_one_initialized_dictionary() {
    mod concurrent {
        zencan_node::include_modules!(EXAMPLE1);
    }
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let od = concurrent::get_od();
                assert_eq!(od.object1018().get_vendor(), 1234);
                assert!(od.od_table().windows(2).all(|w| w[0].index < w[1].index));
                od as *const _ as usize
            })
        })
        .collect();
    let addresses: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(addresses.iter().all(|p| *p == addresses[0]));
}

#[test]
#[serial]
fn repeat_reset_preserves_references_identity_and_scope() {
    let od = get_od();
    let array = od.object2000();
    let record = od.object2001();
    od.object1018().set_serial(0x12345678);
    for _ in 0..3 {
        array.set(0, 999).unwrap();
        record.set_sub3(-1);
        od.object2002().value.set_str(b"longer string!!").unwrap();
        od.object1017().set_value(91);
        od.reset_objects(ResetScope::Communication);
        assert_eq!(array.get(0).unwrap(), 999);
        assert_eq!(od.object1017().get_value(), 0);
        od.reset_objects(ResetScope::Application);
        assert_eq!(array.get(0).unwrap(), 123);
        assert_eq!(array.get(1).unwrap(), u32::MAX);
        assert_eq!(record.get_sub1(), 140);
        assert_eq!(record.get_sub3(), 0x20);
        assert_eq!(od.object2002().get_value(), *b"Some String\0\0\0\0\0");
        assert_eq!(od.object1018().get_serial(), 0x12345678);
        assert!(std::ptr::eq(array, od.object2000()));
    }
}

#[test]
#[serial]
fn reset_restores_every_numeric_wire_type() {
    let od = get_od();
    od.reset_objects(ResetScope::Application);
    for object in [
        od.object300b() as &dyn ObjectAccess,
        od.object300c(),
        od.object300d(),
    ] {
        let mut defaults = Vec::new();
        for sub in 0..=object.max_sub_number() {
            let Ok(info) = object.sub_info(sub) else {
                continue;
            };
            if !info.access_type.is_writable() {
                continue;
            }
            let mut before = vec![0; info.size()];
            object.read(sub, 0, &mut before).unwrap();
            object.write(sub, &vec![1; info.size()]).unwrap();
            defaults.push((sub, before));
        }
        od.reset_objects(ResetScope::Application);
        for (sub, expected) in defaults {
            let mut actual = vec![0; expected.len()];
            object.read(sub, 0, &mut actual).unwrap();
            assert_eq!(actual, expected, "subindex {sub}");
        }
    }
}

#[test]
fn sparse_large_array_and_domain_array_reset() {
    mod sparse {
        zencan_node::build_object_dict!(
            r#"
            device_name = "reset fixture"
            [identity]
            vendor_id = 1
            product_code = 1
            revision_number = 1
            [pdos]
            num_rpdo = 0
            num_tpdo = 0
            [[objects]]
            index = 0x4000
            parameter_name = "Sparse defaults"
            object_type = "array"
            data_type = "uint32"
            access_type = "rw"
            array_size = 128
            default_value = [23, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            [[objects]]
            index = 0x4001
            parameter_name = "External domains"
            object_type = "array"
            data_type = "domain"
            access_type = "rw"
            array_size = 2
        "#
        );
    }
    let od = sparse::get_od();
    static HANDLER: zencan_node::object_dict::ByteField<1> =
        zencan_node::object_dict::ByteField::new([42]);
    od.object4001().array[1].register_handler(&HANDLER);
    for _ in 0..2 {
        for i in 0..128 {
            od.object4000().set(i, 100).unwrap();
        }
        od.reset_objects(ResetScope::Application);
        assert_eq!(od.object4000().get(0).unwrap(), 23);
        for i in 1..128 {
            assert_eq!(od.object4000().get(i).unwrap(), 0);
        }
        assert_eq!(od.object4001().read_u8(2).unwrap(), 42);
    }
}
