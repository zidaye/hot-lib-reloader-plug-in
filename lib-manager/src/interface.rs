use crate::entry::plugin::PluginId;
use abi_stable::std_types::RHashMap;
use abi_stable::{
    sabi_trait,
    sabi_types::RMut,
    std_types::{RResult, RStr, RString},
};
use common_define::error::HotReloaderError;

pub type PluginManagerMut<'a> = PluginManager_TO<'a, RMut<'a, ()>>;

#[sabi_trait]
pub trait PluginManager {
    /// Gets the PluginId of all loaded plugins
    fn loaded_plugins(
        &mut self,
        plugin_name: &RStr,
        monitor_path: RString,
        loading_path: RString,
        load_counter: usize,
    ) -> RResult<PluginId, HotReloaderError>;

    /// Gets the PluginId of all loaded plugins
    fn unloaded_plugins(&mut self, plugin_name: &RStr) -> RResult<PluginId, HotReloaderError>;

    /// Gets the PluginId of all loaded plugins
    fn get_plugins(&self) -> RHashMap<RString, PluginId>;
}
