pub mod object_dict1 {
    zencan_node::include_modules!(EXAMPLE1);
}
pub mod object_dict2 {
    zencan_node::include_modules!(EXAMPLE2);
}
pub mod object_dict3 {
    zencan_node::include_modules!(EXAMPLE3);
}
pub mod sim_bus;
pub mod utils;

pub mod prelude {
    pub use super::sim_bus::{SimBus, SimBusReceiver, SimBusSender};
    pub use super::utils::{get_sdo_client, test_with_background_process, BusLogger};
    pub use zencan_client::{RawAbortCode, SdoClientError};
    pub use zencan_common::{sdo::AbortCode, NodeId};
    pub use zencan_node::{Callbacks, Node};
}
