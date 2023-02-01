use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::HotReloaderError;

pub fn watched_and_loaded_library_paths(
    lib_dir: impl AsRef<Path>,
    lib_name_vec: &Vec<impl AsRef<str>>,
    load_counter: usize,
    need_prefix: bool,
) -> Vec<(PathBuf, PathBuf, String)> {
    let mut file_mapping = vec![];
    let lib_dir = &lib_dir.as_ref();
    // sort out os dependent file name
    #[cfg(target_os = "macos")]
    let (prefix, ext) = ("lib", "dylib");
    #[cfg(target_os = "linux")]
    let (prefix, ext) = ("lib", "so");
    #[cfg(target_os = "windows")]
    let (prefix, ext) = ("", "dll");
    for lib_name in lib_name_vec {
        let lib_name = if need_prefix {
            format!("{prefix}{}", lib_name.as_ref())
        } else {
            lib_name.as_ref().into()
        };
        let watched_lib_file = lib_dir.join(&lib_name).with_extension(ext);
        let loaded_lib_file = lib_dir
            .join(format!("{lib_name}-hot-{load_counter}"))
            .with_extension(ext);
        file_mapping.push((watched_lib_file, loaded_lib_file, lib_name));
    }
    file_mapping
}

/// Try to find that might be a relative path such as `target/debug/` by walking
/// up the directories, starting from cwd. This helps finding the lib when the
/// app was started from a directory that is not the project/workspace root.
pub fn find_file_or_dir_in_parent_directories(
    file: impl AsRef<Path>,
) -> Result<PathBuf, HotReloaderError> {
    let mut file = file.as_ref().to_path_buf();
    if !file.exists() && file.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            let mut parent_dir = Some(cwd.as_path());
            while let Some(dir) = parent_dir {
                if dir.join(&file).exists() {
                    file = dir.join(&file);
                    break;
                }
                parent_dir = dir.parent();
            }
        }
    }

    if file.exists() {
        Ok(file)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("file {file:?} does not exist"),
        )
        .into())
    }
}

pub fn hash_file(f: impl AsRef<Path>) -> u32 {
    fs::read(f.as_ref())
        .map(|content| crc32fast::hash(&content))
        .unwrap_or_default()
}

pub fn get_lib_name_from_path(file: impl AsRef<Path>) -> Option<String> {
    let file = file.as_ref().to_path_buf();
    // sort out os dependent file name
    #[cfg(target_os = "macos")]
    let (prefix, ext) = ("lib", "dylib");
    #[cfg(target_os = "linux")]
    let (prefix, ext) = ("lib", "so");
    #[cfg(target_os = "windows")]
    let (prefix, ext) = ("", "dll");

    if let Some(file_name) = file.file_name() {
        if let Some(file_ext) = file.extension() {
            let mut file_name = file_name.to_string_lossy().to_string();
            if file_ext.eq(ext) && file_name.starts_with(prefix) && !file_name.contains("-hot-") {
                file_name = file_name.replacen(prefix, "", 1);
                file_name = file_name.replacen(&format!(".{}", ext), "", 1);
                return Some(file_name);
            }
        }
    }
    None
}
