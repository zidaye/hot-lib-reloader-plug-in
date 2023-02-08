use std::collections::HashMap;

use crate::entry::plugin::PluginId;
use common_define::error::HotReloaderError;

pub trait PluginManager {
    /// Gets the PluginId of all loaded plugins
    fn loaded_plugins(
        &mut self,
        plugin_name: String,
        monitor_path: String,
        loading_path: String,
        load_counter: usize,
    ) -> Result<PluginId, HotReloaderError>;

    /// Gets the PluginId of all loaded plugins
    fn unloaded_plugins(&mut self, plugin_name: String) -> Result<PluginId, HotReloaderError>;

    /// Gets the PluginId of all loaded plugins
    fn get_plugins(&self) -> HashMap<String, PluginId>;
}
