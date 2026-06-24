use std::collections::BTreeMap;

use crate::object_model::{AccessType, DataType, PdoMappable};

/// A default value for an object dictionary value.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "std", derive(serde::Deserialize), serde(untagged))]
pub enum DefaultValue {
    /// A default value for integer fields.
    Integer(i64),
    /// A default value for float fields.
    Float(f64),
    /// A default value for string-like fields.
    String(String),
}

impl From<i64> for DefaultValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for DefaultValue {
    fn from(value: i32) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<usize> for DefaultValue {
    fn from(value: usize) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<f64> for DefaultValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<&str> for DefaultValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

/// Specification for a sub-object within a record object
#[derive(Clone, Debug, PartialEq)]
pub struct SubObjectSpec {
    /// The name of the parameter stored in the sub object
    pub parameter_name: String,
    /// The data type stored in the sub object
    pub data_type: DataType,
    /// Defines what accesses are allowed on the sub object
    pub access_type: AccessType,
    /// The initial value for the sub object.
    pub default_value: Option<DefaultValue>,
    /// What types of PDOs can the sub object be mapped to
    pub pdo_mapping: PdoMappable,
    /// Should the sub object be stored to persistent storage
    pub persist: bool,
}

/// Defines the properties of the object content
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectShape {
    /// A variable object contains a single value at sub index 0
    Var(VarSpec),
    /// An array object contains N elements of the same data type at sub indices 1-N
    Array(ArraySpec),
    /// A record object contains one or more sub objects of varying types
    Record(RecordSpec),
}

/// Specification for a var object
#[derive(Clone, Debug, PartialEq)]
pub struct VarSpec {
    /// Specification for sub0 (the variable value)
    pub value: SubObjectSpec,
}

/// Specification for an array object
#[derive(Clone, Debug, PartialEq)]
pub struct ArraySpec {
    /// The specification for the first sub object
    ///
    /// This typically is a u8, which gives the number of elements in the array / the highest sub
    /// index. In some cases, it may be writeable.
    sub0: SubObjectSpec,
    /// The specification for the elements in the sub object
    ///
    /// All elements in the array must have the same shape, but
    elements: Vec<SubObjectSpec>,
}

impl ArraySpec {
    /// Create an array spec from a list of elements
    ///
    /// All elements must have the same DataType, but other fields may vary.
    ///
    /// Panics if all data types are not the same, or if elements.len() == 0
    pub fn new(elements: Vec<SubObjectSpec>) -> Self {
        assert!(!elements.is_empty());
        let dt = elements[0].data_type;
        assert!(elements.iter().all(|e| e.data_type == dt));

        Self::new_custom_sub0(
            SubObjectSpec {
                parameter_name: "Max Sub".to_string(),
                data_type: DataType::UInt8,
                access_type: AccessType::Ro,
                default_value: Some(elements.len().into()),
                pdo_mapping: PdoMappable::None,
                persist: false,
            },
            elements,
        )
    }

    /// Create an array spec using custom sub0 attributes
    ///
    /// This can be used, for example, to create a writable sub0
    ///
    /// The datatype for sub0 must always be UInt8.
    ///
    /// Will panic if sub0 data_type is incorrect, if all elements do not have
    /// the same data_type, or if elements.len() == 0.
    pub fn new_custom_sub0(sub0: SubObjectSpec, elements: Vec<SubObjectSpec>) -> Self {
        assert!(sub0.data_type == DataType::UInt8);
        assert!(!elements.is_empty());
        let dt = elements[0].data_type;
        assert!(elements.iter().all(|e| e.data_type == dt));

        Self { sub0, elements }
    }

    /// Get the list of element specifications for the array
    ///
    /// All elements are guaranteed to have the same data type, but otherwise may differ
    pub fn elements(&self) -> &[SubObjectSpec] {
        &self.elements
    }

    /// Get the specification of the sub0 sub-object containing the max sub number
    pub fn sub0_spec(&self) -> &SubObjectSpec {
        &self.sub0
    }

    /// Whether the array has no elements.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Get the length of the array.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Get the data type of the array elements
    pub fn data_type(&self) -> DataType {
        // elements is never allowed to be empty
        self.elements[0].data_type
    }
}

/// Specification for a record object
#[derive(Clone, Debug, PartialEq)]
pub struct RecordSpec {
    /// Collection of sub index -> specification mappings defining the sub-objects within the record
    pub subs: BTreeMap<u8, SubObjectSpec>,
}

#[derive(Clone, Debug, PartialEq)]
/// Specification for an object
pub struct ObjectSpec {
    /// The index used for identifying the object
    pub index: u16,
    /// A human-readable name for the object
    pub parameter_name: String,
    /// The "shape" of the object, specifying what type of object it is, what it contains, and how it i
    pub shape: ObjectShape,
}
