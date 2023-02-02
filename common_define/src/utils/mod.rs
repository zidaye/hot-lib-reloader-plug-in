pub mod path_handler;

#[derive(Debug, Clone)]
pub enum PluginLibEvent {
    Create(String),
    Remove(String),
    Other,
}
