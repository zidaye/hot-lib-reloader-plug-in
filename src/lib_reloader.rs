// use common_define::utils::path_handler::find_file_or_dir_in_parent_directories;
// use common_define::utils::path_handler::watched_and_loaded_library_paths;
// use libloading::Library;
// use libloading::Symbol;
// use notify::watcher;
// use notify::DebouncedEvent;
// use notify::RecursiveMode;
// use notify::Watcher;
// use std::collections::HashMap;
// use std::fs;
// use std::path::{Path, PathBuf};
// use std::sync::atomic::AtomicBool;
// use std::sync::atomic::AtomicU32;
// use std::sync::atomic::Ordering;
// use std::sync::Mutex;
// use std::sync::{mpsc, Arc};
// use std::thread;
// use std::time::Duration;

// use common_define::error::HotReloaderError;

// #[cfg(feature = "verbose")]
// use log;

// /// Manages watches a library (dylib) file, loads it using
// /// [`libloading::Library`] and [provides access to its
// /// symbols](HotLoadingManager::get_symbol). When the library changes, [`HotLoadingManager`]
// /// is able to unload the old version and reload the new version through
// /// [`HotLoadingManager::update`].
// ///
// /// Note that the [`HotLoadingManager`] itself will not actively update, i.e. does not
// /// manage an update thread calling the update function. This is normally
// /// managed by the [`hot_lib_reloader_macro::hot_module`] macro that also
// /// manages the [about-to-load and load](crate::LibReloadNotifier) notifications.
// ///
// /// It can load symbols from the library with [HotLoadingManager::get_symbol].
// pub struct HotLoadingManager {
//     load_counter: HashMap<String, usize>,
//     lib_dir: PathBuf,
//     _lib_name: Vec<String>,
//     // <lib_name, libload_file_changed>
//     changed: HashMap<String, Arc<AtomicBool>>,
//     lib: HashMap<String, Library>,
//     // <lib_name, lib_file_path>
//     watched_lib_file: HashMap<String, PathBuf>,
//     // <lib_name, libload_file_path>
//     loaded_lib_file: HashMap<String, PathBuf>,
//     // <lib_name, libload_file_hash>
//     lib_file_hash: HashMap<String, Arc<AtomicU32>>,
//     file_change_subscribers: Arc<Mutex<Vec<mpsc::Sender<String>>>>,
//     #[cfg(target_os = "macos")]
//     codesigner: crate::codesign::CodeSigner,
// }

// impl HotLoadingManager {
//     /// Creates a HotLoadingManager.
//     ///  `lib_dir` is expected to be the location where the library to use can
//     /// be found. Probably `target/debug` normally.
//     /// `lib_name` is the name of the library, not(!) the file name. It should
//     /// normally be just the crate name of the cargo project you want to hot-reload.
//     /// HotLoadingManager will take care to figure out the actual file name with
//     /// platform-specific prefix and extension.
//     pub fn new(
//         lib_dir: impl AsRef<Path>,
//         lib_name: impl AsRef<str>,
//         file_watch_debounce: Option<Duration>,
//     ) -> Result<Self, HotReloaderError> {
//         // 感觉用不上，暂时注释
//         // compromise::register!();
//         // compromise::set_hot_reload_enabled(true);
//         // find the target dir in which the build is happening and where we should find
//         // the library
//         let lib_dir = find_file_or_dir_in_parent_directories(lib_dir.as_ref())?;
//         log::debug!("found lib dir at {lib_dir:?}");

//         let init_load_counter = 0;

//         #[cfg(target_os = "macos")]
//         let codesigner = crate::codesign::CodeSigner::new();

//         let mut watched_lib_file = HashMap::new();
//         let mut loaded_lib_file = HashMap::new();
//         let mut lib = HashMap::new();
//         let mut changed = HashMap::new();
//         let mut lib_file_hash = HashMap::new();
//         let file_change_subscribers = Arc::new(Mutex::new(Vec::new()));
//         let mut load_counter = HashMap::new();

//         for (current_watched_lib_file, current_loaded_lib_file, current_lib_name) in file_mapping {
//             let (current_lib_file_hash, current_lib) = if current_watched_lib_file.exists() {
//                 // We don't load the actual lib because this can get problems e.g. on Windows
//                 // where a file lock would be held, preventing the lib from changing later.
//                 log::debug!("copying {current_watched_lib_file:?} -> {current_loaded_lib_file:?}");
//                 fs::copy(&current_watched_lib_file, &current_loaded_lib_file)?;
//                 let hash = hash_file(&current_loaded_lib_file);
//                 #[cfg(target_os = "macos")]
//                 codesigner.codesign(&loaded_lib_file);
//                 (hash, load_library(&current_loaded_lib_file)?)
//             } else {
//                 log::debug!("library {current_watched_lib_file:?} does not yet exist");
//                 continue;
//             };

//             let current_lib_file_hash = Arc::new(AtomicU32::new(current_lib_file_hash));
//             let current_changed = Arc::new(AtomicBool::new(false));

//             Self::watch(
//                 current_lib_name.clone(),
//                 current_watched_lib_file.clone(),
//                 current_lib_file_hash.clone(),
//                 current_changed.clone(),
//                 file_change_subscribers.clone(),
//                 file_watch_debounce
//                     .clone()
//                     .unwrap_or_else(|| Duration::from_millis(500)),
//             )?;
//             watched_lib_file.insert(current_lib_name.clone(), current_watched_lib_file);
//             loaded_lib_file.insert(current_lib_name.clone(), current_loaded_lib_file);
//             lib.insert(current_lib_name.clone(), current_lib);
//             changed.insert(current_lib_name.clone(), current_changed);
//             lib_file_hash.insert(current_lib_name.clone(), current_lib_file_hash);
//             load_counter.insert(current_lib_name, init_load_counter);
//         }

//         let lib_loader = Self {
//             load_counter,
//             lib_dir,
//             _lib_name: lib_name_vec,
//             watched_lib_file,
//             loaded_lib_file,
//             lib,
//             lib_file_hash,
//             changed,
//             file_change_subscribers,
//             #[cfg(target_os = "macos")]
//             codesigner,
//         };

//         Ok(lib_loader)
//     }

//     // needs to be public as it is used inside the hot_module macro.
//     #[doc(hidden)]
//     pub fn subscribe_to_file_changes(&mut self) -> mpsc::Receiver<String> {
//         log::trace!("subscribe to file change");
//         let (tx, rx) = mpsc::channel();
//         let mut subscribers = self.file_change_subscribers.lock().unwrap();
//         subscribers.push(tx);
//         rx
//     }

//     /// Checks if the watched library has changed. If it has, reload it and return
//     /// true. Otherwise return false.
//     pub fn update(&mut self, lib_name: impl AsRef<str>) -> Result<bool, HotReloaderError> {
//         if let Some(currentt_changed) = self.changed.get(lib_name.as_ref()) {
//             if !currentt_changed.load(Ordering::Acquire) {
//                 return Ok(false);
//             }
//             currentt_changed.store(false, Ordering::Release);

//             self.reload(lib_name)?;
//         }
//         Ok(true)
//     }

//     /// Reload library `self.lib_file`.
//     fn reload(&mut self, current_lib_name: impl AsRef<str>) -> Result<(), HotReloaderError> {
//         let Self {
//             load_counter,
//             lib_dir,
//             watched_lib_file,
//             loaded_lib_file,
//             ..
//         } = self;

//         log::info!("reloading lib {watched_lib_file:?}");

//         // Close the loaded lib, copy the new lib to a file we can load, then load it.

//         if let Some(lib) = self.lib.remove(current_lib_name.as_ref()) {
//             lib.close()?;
//             if let Some(current_loaded_lib_file) = loaded_lib_file.get(current_lib_name.as_ref()) {
//                 if current_loaded_lib_file.exists() {
//                     let _ = fs::remove_file(current_loaded_lib_file);
//                 }
//             }
//         }

//         if let Some(current_watched_lib_file) = watched_lib_file.get(current_lib_name.as_ref()) {
//             if current_watched_lib_file.exists() {
//                 let current_load_counter = if let Some(current_load_counter) =
//                     load_counter.get_mut(current_lib_name.as_ref())
//                 {
//                     *current_load_counter += 1;
//                     *current_load_counter
//                 } else {
//                     let current_load_counter = 0;
//                     load_counter.insert(current_lib_name.as_ref().into(), current_load_counter);
//                     current_load_counter
//                 };

//                 let lib_file_mapping = watched_and_loaded_library_paths(
//                     lib_dir,
//                     &vec![current_lib_name.as_ref()],
//                     current_load_counter,
//                     false,
//                 );
//                 if let Some((current_watched_lib_file, current_loaded_lib_file, _)) =
//                     lib_file_mapping.first()
//                 {
//                     log::trace!("copy {current_watched_lib_file:?} -> {current_loaded_lib_file:?}");
//                     fs::copy(current_watched_lib_file, &current_loaded_lib_file)?;
//                     let current_lib_file_hash_value = hash_file(current_loaded_lib_file);
//                     if let Some(current_lib_file_hash) =
//                         self.lib_file_hash.get(current_lib_name.as_ref())
//                     {
//                         current_lib_file_hash.store(current_lib_file_hash_value, Ordering::Release);
//                     } else {
//                         let current_lib_file_hash =
//                             Arc::new(AtomicU32::new(current_lib_file_hash_value));
//                         self.lib_file_hash
//                             .insert(current_lib_name.as_ref().into(), current_lib_file_hash);
//                         let current_changed = Arc::new(AtomicBool::new(false));
//                         self.changed
//                             .insert(current_lib_name.as_ref().into(), current_changed);
//                     }

//                     #[cfg(target_os = "macos")]
//                     self.codesigner.codesign(&loaded_lib_file);
//                     self.lib.insert(
//                         current_lib_name.as_ref().into(),
//                         load_library(current_loaded_lib_file)?,
//                     );
//                     self.loaded_lib_file.insert(
//                         current_lib_name.as_ref().into(),
//                         current_loaded_lib_file.to_path_buf(),
//                     );
//                 }
//             } else {
//                 log::warn!("trying to reload library but it does not exist");
//             }
//         }

//         Ok(())
//     }

//     /// Watch for changes of `lib_file`.
//     fn watch(
//         current_lib_name: String,
//         lib_file: impl AsRef<Path>,
//         lib_file_hash: Arc<AtomicU32>,
//         changed: Arc<AtomicBool>,
//         file_change_subscribers: Arc<Mutex<Vec<mpsc::Sender<String>>>>,
//         debounce: Duration,
//     ) -> Result<(), HotReloaderError> {
//         let lib_file = lib_file.as_ref().to_path_buf();
//         log::info!("start watching changes of file {}", lib_file.display());

//         // File watcher thread. We watch `self.lib_file`, when it changes and we haven't
//         // a pending change still waiting to be loaded, set `self.changed` to true. This
//         // then gets picked up by `self.update`.
//         thread::spawn(move || {
//             use DebouncedEvent::*;

//             let (tx, rx) = mpsc::channel();
//             let mut watcher = watcher(tx, debounce).unwrap();
//             watcher
//                 .watch(&lib_file, RecursiveMode::NonRecursive)
//                 .expect("watch lib file");

//             let signal_change = || {
//                 if hash_file(&lib_file) == lib_file_hash.load(Ordering::Acquire)
//                     || changed.load(Ordering::Acquire)
//                 {
//                     // file not changed
//                     return false;
//                 }

//                 log::debug!("{lib_file:?} changed",);

//                 changed.store(true, Ordering::Release);

//                 // inform subscribers
//                 let subscribers = file_change_subscribers.lock().unwrap();
//                 log::trace!(
//                     "sending ChangedEvent::LibFileChanged to {} subscribers",
//                     subscribers.len()
//                 );
//                 for tx in &*subscribers {
//                     let _ = tx.send(current_lib_name.clone());
//                 }

//                 true
//             };

//             loop {
//                 match rx.recv() {
//                     Ok(Chmod(_) | Create(_) | Write(_)) => {
//                         signal_change();
//                     }
//                     Ok(Remove(_)) => {
//                         // just one hard link removed?
//                         if !lib_file.exists() {
//                             log::debug!(
//                                 "{} was removed, trying to watch it again...",
//                                 lib_file.display()
//                             );
//                         }
//                         loop {
//                             if watcher
//                                 .watch(&lib_file, RecursiveMode::NonRecursive)
//                                 .is_ok()
//                             {
//                                 log::info!("watching {lib_file:?} again after removal");
//                                 signal_change();
//                                 break;
//                             }
//                             thread::sleep(Duration::from_millis(500));
//                         }
//                     }
//                     Ok(change) => {
//                         log::trace!("file change event: {change:?}");
//                     }
//                     Err(err) => {
//                         log::error!("file watcher error, stopping reload loop: {err}");
//                         break;
//                     }
//                 }
//             }
//         });

//         Ok(())
//     }

//     /// Get a pointer to a function or static variable by symbol name. Just a
//     /// wrapper around [libloading::Library::get].
//     ///
//     /// The `symbol` may not contain any null bytes, with the exception of the
//     /// last byte. Providing a null-terminated `symbol` may help to avoid an
//     /// allocation. The symbol is interpreted as is, no mangling.
//     ///
//     /// # Safety
//     ///
//     /// Users of this API must specify the correct type of the function or variable loaded.
//     pub unsafe fn get_symbol<T>(
//         &self,
//         lib_name: impl AsRef<str>,
//         name: &[u8],
//     ) -> Result<Symbol<T>, HotReloaderError> {
//         log::trace!(
//             "Call function with get_symbol: {:?}, lib name: {}",
//             String::from_utf8(name.to_vec())
//                 .map_err(|e| HotReloaderError::LibraryloadAccidentError(e.to_string().into())),
//             lib_name.as_ref()
//         );
//         let specify_ilb = self.lib.get(lib_name.as_ref());
//         match specify_ilb {
//             None => Err(HotReloaderError::LibraryNotLoaded),
//             Some(lib) => Ok(lib.get(name)?),
//         }
//     }

//     /// Helper to log from the macro without requiring the user to have the log
//     /// crate around
//     #[doc(hidden)]
//     pub fn log_info(what: impl std::fmt::Display) {
//         log::info!("{}", what);
//     }
// }

// /// Deletes the currently loaded lib file if it exists
// impl Drop for HotLoadingManager {
//     fn drop(&mut self) {
//         for (_, file_path) in &self.loaded_lib_file {
//             if file_path.exists() {
//                 log::trace!("removing {:?}", file_path);
//                 let _ = fs::remove_file(file_path);
//             }
//         }
//     }
// }

// fn load_library(lib_file: impl AsRef<Path>) -> Result<Library, HotReloaderError> {
//     Ok(unsafe { Library::new(lib_file.as_ref()) }?)
// }
