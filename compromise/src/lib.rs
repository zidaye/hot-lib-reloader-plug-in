
#[cfg(target_os = "linux")]
pub mod linux;

use std::{sync::atomic::AtomicBool, sync::atomic::Ordering};

static HOT_RELOAD_ENABLED: AtomicBool = AtomicBool::new(false);

// this one will be called from our executable, so it needs to be `pub`
pub fn set_hot_reload_enabled(enabled: bool) {
    HOT_RELOAD_ENABLED.store(enabled, Ordering::SeqCst)
}

// this one can be `pub(crate)`, it'll only be called internally
pub(crate) fn is_hot_reload_enabled() -> bool {
    HOT_RELOAD_ENABLED.load(Ordering::SeqCst)
}

#[macro_export]
macro_rules! register {
    () => {
        use std::ffi::c_void;
        #[cfg(target_os = "linux")]
        #[no_mangle]
        pub unsafe extern "C" fn __cxa_thread_atexit_impl(
            func: *mut c_void,
            obj: *mut c_void,
            dso_symbol: *mut c_void,
        ) {
            compromise::linux::thread_atexit(func, obj, dso_symbol);
        }
    };
}