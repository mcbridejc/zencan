use std::collections::BTreeMap;

use zencan_common::object_model::{
    ArraySpec, ObjectShape, ObjectSpec, RecordSpec, SubObjectSpec, VarSpec,
};

/// Enumeration of the possible object implementation strategies
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectImplementation {
    /// An object implemented by zencan-node
    System(SystemObjectKind),
    /// A standard custom object generated from a device config description
    Generated,
    /// An application callback object
    ///
    /// This is a placeholder in the object dictionary where the application can register its own
    /// handlers
    Callback,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// The possible ways to implement a record sub-object on a generated object
pub enum SubObjectImplemention {
    Storage,
    Callback,
}

/// Collection of the possible system objects
#[derive(Clone, Debug, PartialEq)]
pub enum SystemObjectKind {
    Identity {
        vendor: u32,
        product: u32,
        revision: u32,
    },
    Pdo,
    StorageCommand,
    BootloaderInfo {
        application: bool,
        num_sections: usize,
    },
    BootloaderSection {
        name: String,
        size: u32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubObjectBuildSpec {
    /// The specification of the sub object format
    pub spec: SubObjectSpec,
    /// Whether the subject object is implemented as an application callback or storage
    pub implementation: SubObjectImplemention,
    /// The field name to use for the object
    pub field_name: Option<String>,
}

impl From<&SubObjectBuildSpec> for SubObjectSpec {
    fn from(value: &SubObjectBuildSpec) -> Self {
        value.spec.clone()
    }
}

/// A build specification for record objects
///
/// These differ from the public spec in `object_spec` because for records we have to keep track of
/// which sub objects are implemented as application callbacks
#[derive(Clone, Debug, PartialEq)]
pub struct RecordBuildSpec {
    pub subs: BTreeMap<u8, SubObjectBuildSpec>,
}

impl From<&RecordBuildSpec> for RecordSpec {
    fn from(value: &RecordBuildSpec) -> Self {
        RecordSpec {
            subs: value
                .subs
                .iter()
                .map(|(index, item)| (*index, item.into()))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectBuildShape {
    Var(VarSpec),
    Array(ArraySpec),
    Record(RecordBuildSpec),
}

impl From<&ObjectBuildShape> for ObjectShape {
    fn from(value: &ObjectBuildShape) -> Self {
        match value {
            ObjectBuildShape::Var(var_spec) => ObjectShape::Var(var_spec.clone()),
            ObjectBuildShape::Array(array_spec) => ObjectShape::Array(array_spec.clone()),
            ObjectBuildShape::Record(record_build_spec) => {
                ObjectShape::Record(record_build_spec.into())
            }
        }
    }
}

impl From<ObjectBuildShape> for ObjectShape {
    fn from(value: ObjectBuildShape) -> Self {
        (&value).into()
    }
}

/// Specification for building an object implementation
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectBuildSpec {
    pub index: u16,
    pub parameter_name: String,
    pub object_name: String,
    pub shape: ObjectBuildShape,
    pub implementation: ObjectImplementation,
}

impl From<&ObjectBuildSpec> for ObjectSpec {
    fn from(value: &ObjectBuildSpec) -> Self {
        ObjectSpec {
            index: value.index,
            parameter_name: value.parameter_name.clone(),
            shape: value.shape.clone().into(),
        }
    }
}

impl From<ObjectBuildSpec> for ObjectSpec {
    fn from(value: ObjectBuildSpec) -> Self {
        (&value).into()
    }
}
