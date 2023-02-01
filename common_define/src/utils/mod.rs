use abi_stable::std_types::RString;

pub mod path_handler;

#[derive(Debug, Clone)]
pub enum PluginLibEvent {
    Create(RString),
    Remove(RString),
    Other,
}
