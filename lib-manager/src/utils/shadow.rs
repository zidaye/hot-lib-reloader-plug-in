use libc::gnu_get_libc_version;
use shadow_rs::shadow;
shadow!(shadow);

pub const fn get_rust_version() -> &'static str {
    shadow::RUST_VERSION
}

pub const fn get_rust_channel() -> &'static str {
    shadow::RUST_CHANNEL
}

pub const fn get_cargo_version() -> &'static str {
    shadow::CARGO_VERSION
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn get_libc_version() -> String {
    use std::ffi::CStr;
    let version_cstr = unsafe { CStr::from_ptr(gnu_get_libc_version()) };
    version_cstr.to_str().unwrap_or_default().into()
}

pub const fn hot_lib_version_col() -> &'static str {
    const_format::formatcp!(
        "{}_{}_{}",
        get_rust_channel(),
        get_rust_version(),
        get_cargo_version()
    )
}
