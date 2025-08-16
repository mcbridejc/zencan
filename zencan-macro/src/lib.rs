// use eds_parser::ElectronicDataSheet;

extern crate proc_macro;
use std::str::FromStr as _;
use zencan_build::device_config_to_string;
use zencan_common::device_config::DeviceConfig;

/// Macro to build an object dict from inline device config TOML
#[proc_macro]
pub fn build_object_dict(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let str_literal: syn::LitStr =
        syn::parse(input).expect("Expected string literal as macro argument");
    let device =
        DeviceConfig::load_from_str(&str_literal.value()).expect("Error parsing device config");
    proc_macro::TokenStream::from_str(&device_config_to_string(&device, true).unwrap()).unwrap()
}
