#[macro_export]
macro_rules! hotlib_version_strings {
    () => {{
        use abi_stable::sabi_types::VersionStrings;
        use hot_lib_reloader_plug_in::lib_manager::utils::shadow::hot_lib_version_col;
        VersionStrings::new(hot_lib_version_col())
    }};
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_hotlib_version_strings() {
        println!("{}", hotlib_version_strings!())
    }
}
