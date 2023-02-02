use abi_stable::std_types::RString;
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, Eq, abi_stable::StableAbi, Serialize, Deserialize)]
pub struct PluginId {
    pub named: RString,
    /// The number of the instance of this Plugin.
    pub path: RString,
    pub del_flog: bool,
}

#[macro_export(local_inner_macros)]
macro_rules! plugin_object_define {
    ($trait_:ident, $prefix_ref:ident, $plugin_type:ident) => {
        use abi_stable::sabi_trait::reexports::__sabi_re::RBox;
        use abi_stable::std_types::RResult;
        use abi_stable::utils;
        use hot_lib_reloader_plug_in::common_define::error::HotReloaderError;
        pub type PluginType = $plugin_type<'static, RBox<()>>;

        #[repr(C)]
        #[derive(StableAbi)]
        #[sabi(kind(Prefix(prefix_ref = $prefix_ref)))]
        #[sabi(missing_field(panic))]
        pub struct PluginObject {
            #[sabi(last_prefix_field)]
            pub new: extern "C" fn(PluginId) -> RResult<PluginType, HotReloaderError>,
        }

        impl abi_stable::library::RootModule for $prefix_ref {
            abi_stable::declare_root_module_statics! {$prefix_ref}
            const BASE_NAME: &'static str = "plugin";
            const NAME: &'static str = "plugin";
            const VERSION_STRINGS: abi_stable::sabi_types::VersionStrings =
                abi_stable::package_version_strings!();
            // hot_lib_reloader_plug_in::lib_manager::hotlib_version_strings!();
        }
    };
}

// #[sabi_trait]
// pub trait PluginTest: Send {
//     fn plugin_id(&self) -> &PluginId;
//     fn close(self);
// }

// plugin_object_define!(PluginTest, PluginObject_Ref, PluginTest_TO);
