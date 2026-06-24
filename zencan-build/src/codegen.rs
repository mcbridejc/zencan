//! Assemble a dictionary from per-object generators.
use crate::{
    device_config::{DeviceConfig, PdoDefaultConfig},
    errors::CompileError,
    object_build_spec::ObjectImplementation,
    system_objects::{
        CallbackObjectGenerator, CustomObjectGenerator, ObjectGenerator, SystemObjectGenerator,
    },
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

fn pdo_default(cfg: Option<&PdoDefaultConfig>) -> TokenStream {
    if let Some(cfg) = cfg {
        let PdoDefaultConfig {
            cob_id,
            extended,
            add_node_id,
            enabled,
            rtr_disabled,
            transmission_type,
            mappings,
        } = cfg;
        let mappings = mappings.iter().map(|m| m.to_object_value());
        quote!(const { &PdoDefaults::new(#cob_id, #extended, #add_node_id, #enabled, #rtr_disabled, #transmission_type, &[#(#mappings),*]) })
    } else {
        quote!(&PdoDefaults::DEFAULT)
    }
}

fn validate_pdo_defaults(
    dev: &DeviceConfig,
    specs: &[crate::object_build_spec::ObjectBuildSpec],
) -> Result<(), CompileError> {
    use crate::object_build_spec::ObjectBuildShape;
    for (transmit, count, defaults) in [
        (true, dev.pdos.num_tpdo, &dev.pdos.tpdo_defaults),
        (false, dev.pdos.num_rpdo, &dev.pdos.rpdo_defaults),
    ] {
        for (&number, cfg) in defaults {
            let invalid = |reason: &str| CompileError::InvalidPdoDefaults {
                message: format!("{}PDO {number}: {reason}", if transmit { "T" } else { "R" }),
            };
            if number >= count as usize {
                return Err(invalid("slot exceeds configured PDO count"));
            }
            let limit = if cfg.extended { 0x1fff_ffff } else { 0x7ff };
            if cfg
                .cob_id
                .checked_add(if cfg.add_node_id { 127 } else { 0 })
                .is_none_or(|id| id > limit)
            {
                return Err(invalid("COB-ID exceeds the CAN identifier range"));
            }
            if cfg.mappings.len() > 8
                || cfg.mappings.iter().map(|m| m.size as usize).sum::<usize>() > 64
            {
                return Err(invalid("mappings exceed one CAN frame"));
            }
            for mapping in &cfg.mappings {
                let object = specs
                    .iter()
                    .find(|s| s.index == mapping.index)
                    .ok_or_else(|| invalid("mapped object does not exist"))?;
                let sub = match &object.shape {
                    ObjectBuildShape::Var(v) => (mapping.sub == 0).then_some(&v.value),
                    ObjectBuildShape::Array(a) => {
                        if mapping.sub == 0 {
                            Some(a.sub0_spec())
                        } else {
                            a.elements().get(mapping.sub as usize - 1)
                        }
                    }
                    ObjectBuildShape::Record(r) => r.subs.get(&mapping.sub).map(|s| &s.spec),
                }
                .ok_or_else(|| invalid("mapped subobject does not exist"))?;
                let allowed = if transmit {
                    sub.pdo_mapping.supports_tpdo() && sub.access_type.is_readable()
                } else {
                    sub.pdo_mapping.supports_rpdo() && sub.access_type.is_writable()
                };
                if !allowed
                    || mapping.size == 0
                    || mapping.size % 8 != 0
                    || mapping.size as usize / 8 > sub.data_type.size()
                {
                    return Err(invalid("incompatible mapping type, width or access"));
                }
            }
        }
    }
    Ok(())
}

fn generate(dev: &DeviceConfig) -> Result<TokenStream, CompileError> {
    let mut specs = dev.elaborate()?;
    specs.sort_by_key(|s| s.index);
    validate_pdo_defaults(dev, &specs)?;
    let mut definitions = quote!();
    let mut constructors = quote!();
    let mut allocations = quote!();
    let mut getters = quote!();
    let mut entries = vec![];
    let mut resets = quote!();
    for spec in &specs {
        // To the greatest extent possible, generators should handle the differences in how objects
        // are generated, allowing all object types to be treated the same here.
        let generator: Box<dyn ObjectGenerator> = match &spec.implementation {
            ObjectImplementation::Generated => Box::new(CustomObjectGenerator::new(spec.clone())),
            ObjectImplementation::Callback => Box::new(CallbackObjectGenerator::new(spec.clone())),
            ObjectImplementation::System(system_object_kind) => Box::new(
                SystemObjectGenerator::new(spec.clone(), system_object_kind.clone()),
            ),
        };
        // Get types defined by the object
        definitions.extend(generator.type_definitions()?);

        if let Some(instance) = generator.instance()? {
            let var_name = format_ident!("OBJECT{:X}", spec.index);
            let ty = instance.object_type;
            let init = instance.initializer;
            let reset = instance.reset;
            let getter = instance.field_name;

            allocations
                .extend(quote!(static #var_name: StaticStorage<#ty> = StaticStorage::new();));
            constructors.extend(quote!(#var_name.write(#init);));
            getters.extend(quote!(
                #[inline(always)]
                pub fn #getter(&self) -> &'static #ty {
                    // SAFETY: An ObjectDict token is published only after every object is initialized.
                    unsafe { #var_name.assume_init_ref() }
                }
            ));

            if let Some(reset) = reset {
                let reset_tokens = quote! { self.#getter().#reset; };
                let comm = (0x1000..0x2000).contains(&spec.index);
                if comm {
                    resets.extend(reset_tokens);
                } else {
                    resets.extend(
                        quote!(if scope == zencan_node::ResetScope::Application { #reset_tokens }),
                    );
                }
            }
            let index = spec.index;
            entries.push(
                quote!(ODEntryUninit::from_ptr(#index, #var_name.as_ptr() as *const dyn ObjectAccess)),
            );
        }
    }
    let count = entries.len();
    let nr = dev.pdos.num_rpdo as usize;
    let nt = dev.pdos.num_tpdo as usize;
    let rp = (0..nr).map(|i| {
        let defaults = pdo_default(dev.pdos.rpdo_defaults.get(&i));
        quote!(rpdo_ptr.add(#i).write(Pdo::new(&LOOKUP, &NMT, &rpdo_data[#i], #defaults));)
    });
    let tp = (0..nt).map(|i| {
        let defaults = pdo_default(dev.pdos.tpdo_defaults.get(&i));
        quote!(tpdo_ptr.add(#i).write(Pdo::new(&LOOKUP, &NMT, &tpdo_data[#i], #defaults));)
    });
    Ok(quote! {
        #[allow(non_snake_case, unused_imports, dead_code, clippy::redundant_closure)]
        mod objects {
            use zencan_node::{NodeState, NodeMbox, SDO_BUFFER_SIZE};
            use zencan_node::common::{i24, u24, can::CanMessage, protocol::AbortCode};
            use zencan_node::common::object_model::{TimeOfDay, TimeDifference};
            use zencan_node::object_dict::*;
            use zencan_node::pdo::{Pdo, PdoData, PdoDefaults, PdoCommObject, PdoMappingObject};
            use zencan_node::storage::StorageCommandObject;
            use zencan_node::priority_queue::PriorityQueue;
            use zencan_node::init_cell::{InitCell, StaticStorage};
            #definitions
            #allocations
            static DICT: InitCell<ObjectDict> = InitCell::new();
            static RPDO_DATA: StaticStorage<[PdoData<'static>; #nr]> = StaticStorage::new();
            static TPDO_DATA: StaticStorage<[PdoData<'static>; #nt]> = StaticStorage::new();
            static RPDOS: StaticStorage<[Pdo<'static>; #nr]> = StaticStorage::new();
            static TPDOS: StaticStorage<[Pdo<'static>; #nt]> = StaticStorage::new();
            static NODE_STATE: StaticStorage<NodeState<'static>> = StaticStorage::new();
            static NODE_MBOX: StaticStorage<NodeMbox> = StaticStorage::new();
            // This buffer is owned exclusively by the mailbox.
            static mut SDO_BUFFER: [u8; SDO_BUFFER_SIZE] = [0; SDO_BUFFER_SIZE];
            // Only pointer representation bytes are formed here, never references
            // to uninitialized objects. This table requires no runtime writes.
            static OD_TABLE: [ODEntryUninit; #count] = [#(#entries),*];
            static TX_QUEUE: PriorityQueue<4, CanMessage> = PriorityQueue::new();
            static LOOKUP: Lookup = Lookup;
            static NMT: Nmt = Nmt;
            struct Lookup;
            impl ObjectLookup<'static> for Lookup {
                fn find_object(&self, index: u16) -> Option<&'static dyn ObjectAccess> { find_object(get_od().od_table(), index) }
            }
            struct Nmt;
            impl zencan_node::NmtStateAccess for Nmt {
                fn nmt_state(&self) -> zencan_node::common::protocol::NmtState {
                    zencan_node::NmtStateAccess::nmt_state(get_od().node_state())
                }
            }
            /// The generated object dictionary
            ///
            /// Can only be created by `get_od()`, and existence of it proves that the dictionary's
            /// static objects have been initialized.
            pub struct ObjectDict {
                _private: (),
            }
            impl ObjectDict {
                #getters
                #[inline(always)]
                pub fn node_state(&self) -> &'static NodeState<'static> {
                    // SAFETY: This token is available only after initialization.
                    unsafe { NODE_STATE.assume_init_ref() }
                }
                #[inline(always)]
                pub fn node_mbox(&self) -> &'static NodeMbox {
                    // SAFETY: This token is available only after initialization.
                    unsafe { NODE_MBOX.assume_init_ref() }
                }

                /// Restore objects to their default values
                fn reset_defaults(&self, scope: zencan_node::ResetScope) {
                    let _ = scope;
                    #resets
                    for pdo in self.node_state().rpdos().iter().chain(self.node_state().tpdos()) {
                        pdo.init_defaults();
                    }
                }
                #[inline(always)]
                pub fn od_table(&self) -> &'static [ODEntry<'static>] {
                    // SAFETY: The guard publishes all objects before this token.
                    unsafe { ODEntryUninit::assume_init_slice(&OD_TABLE) }
                }
            }

            impl ObjectLookup<'static> for ObjectDict {
                fn find_object(&self, index: u16) -> Option<&'static dyn ObjectAccess> { find_object(self.od_table(), index) }
            }

            impl ObjectDictionary<'static> for ObjectDict {
                fn reset_objects(&self, scope: zencan_node::ResetScope) {
                    self.reset_defaults(scope);
                }

                fn od_table(&self) -> &'static [ODEntry<'static>] {
                    self.od_table()
                }
            }

            /// Get the generated zencan Object Dictionary
            #[inline]
            pub fn get_od() -> &'static ObjectDict {
                match DICT.get() {
                    Some(od) => od,
                    None => init_od(),
                }
            }
            #[cold]
            #[inline(never)]
            fn init_od() -> &'static ObjectDict {
                // SAFETY: The ObjectDict is the token which attests that objects have been
                // initialized. It is not created until after all objects are initialized, and then
                // it provides access to them via accessors. InitCell::get_or_init ensures that no
                // reentrant initialization is allowed, and that if a panic occurs during
                // initialization the cell is poisoned.
                let od = DICT.get_or_init(|| unsafe {

                    let rpdo_data_ptr = RPDO_DATA.as_ptr().cast::<PdoData<'static>>();
                    for i in 0..#nr { rpdo_data_ptr.add(i).write(PdoData::new()); }
                    let rpdo_data: &'static [PdoData<'static>; #nr] = &*rpdo_data_ptr.cast();
                    let tpdo_data_ptr = TPDO_DATA.as_ptr().cast::<PdoData<'static>>();
                    for i in 0..#nt { tpdo_data_ptr.add(i).write(PdoData::new()); }
                    let tpdo_data: &'static [PdoData<'static>; #nt] = &*tpdo_data_ptr.cast();
                    let rpdo_ptr = RPDOS.as_ptr().cast::<Pdo<'static>>();
                    #(#rp)*
                    let rpdos: &'static [Pdo<'static>; #nr] = &*rpdo_ptr.cast();
                    let tpdo_ptr = TPDOS.as_ptr().cast::<Pdo<'static>>();
                    #(#tp)*
                    let tpdos: &'static [Pdo<'static>; #nt] = &*tpdo_ptr.cast();
                    let node_state = NODE_STATE.write(NodeState::new(rpdos, tpdos));
                    #constructors

                    NODE_MBOX.write(NodeMbox::new(
                        rpdos, tpdos, &TX_QUEUE, &mut *core::ptr::addr_of_mut!(SDO_BUFFER),
                    ));
                    ObjectDict { _private: () }
                });
                od.reset_defaults(zencan_node::ResetScope::Application);
                od
            }

        }
        pub use objects::*;
    })
}

/// Generate a node dictionary from a device configuration.
pub fn device_config_to_string(dev: &DeviceConfig, format: bool) -> Result<String, CompileError> {
    let tokens = generate(dev)?;
    if format {
        Ok(prettyplease::unparse(
            &syn::parse2(tokens).expect("invalid generated Rust"),
        ))
    } else {
        Ok(tokens.to_string())
    }
}
