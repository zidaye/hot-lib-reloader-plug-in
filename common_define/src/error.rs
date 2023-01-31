use abi_stable::{std_types::RString, StableAbi};

#[repr(u8)]
#[derive(thiserror::Error, Debug, StableAbi)]
pub enum HotReloaderError {
    #[error("Cannot lock library: {0}")]
    CannotLockLibrary(RString),
    #[error("Unable to copy library file: {0}")]
    LibraryCopyError(RString),
    #[error("Unable to load library: {0}")]
    LibraryLoadError(RString),
    #[error("The hot reloadable library has not been loaded. Has it not been built yet?")]
    LibraryNotLoaded,
    #[error("Library load find accident error: {0}")]
    LibraryloadAccidentError(RString),
    #[error("Library load abi_stable error: {0}")]
    LibraryloadAbiStableError(RString),
}

impl From<std::io::Error> for HotReloaderError {
    fn from(e: std::io::Error) -> Self {
        HotReloaderError::LibraryCopyError(e.to_string().into())
    }
}

impl From<libloading::Error> for HotReloaderError {
    fn from(e: libloading::Error) -> Self {
        HotReloaderError::LibraryLoadError(e.to_string().into())
    }
}

impl From<abi_stable::library::LibraryError> for HotReloaderError {
    fn from(e: abi_stable::library::LibraryError) -> Self {
        HotReloaderError::LibraryloadAbiStableError(e.to_string().into())
    }
}
