#[macro_export(local_inner_macros)]
macro_rules! impl_plugin_manager {
    () => {
        use std::{
            fs,
            path::{Path, PathBuf},
            sync::{
                atomic::{AtomicBool, AtomicU32, Ordering},
                mpsc, Arc, Mutex,
            },
            thread,
            time::Duration,
        };

        use abi_stable::{
            library::{lib_header_from_path},
            std_types::{
                ROk, RErr,
                RString,
                ROption::RSome,
            },
            reexports::SelfOps,
        };
        use std::collections::HashMap;
        use hot_lib_reloader_plug_in::common_define::{
            utils::{
                path_handler::{
                find_file_or_dir_in_parent_directories, hash_file, watched_and_loaded_library_paths,get_lib_name_from_path,
                },
                PluginLibEvent,
            },
        };


        use hot_lib_reloader_plug_in::lib_manager::notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
        use hot_lib_reloader_plug_in::lib_manager::interface::PluginManager;
        use hot_lib_reloader_plug_in::log;
        #[derive(Default)]
        pub struct HotLoadingManager {
            pub plugin_mapping: HashMap<String, Arc<Mutex<PluginType>>>,
            pub plugin_load_counter: HashMap<String, usize>,
            pub plugin_dir: PathBuf,
            // <lib_name, libload_file_changed>
            pub changed_record: HashMap<String, Arc<AtomicBool>>,
            pub monitor_lib_file: HashMap<String, PathBuf>,
            // <lib_name, libload_file_path>
            pub plugin_infos: HashMap<String, PluginId>,
            // <lib_name, libload_file_hash>
            pub lib_file_hash: HashMap<String, Arc<AtomicU32>>,
            pub file_change_subscribers: Arc<Mutex<Vec<mpsc::Sender<(String, PluginLibEvent)>>>>,
            pub monitor_debounce: Option<Duration>,
        }

        impl HotLoadingManager {
            pub fn new(
                lib_dir: impl AsRef<Path>,
                lib_name: impl AsRef<str>,
                monitor_debounce: Option<Duration>,
            ) -> Result<HotLoadingManager, HotReloaderError> {
                let mut hot_loading_manager = HotLoadingManager {
                    plugin_mapping: HashMap::new(),
                    plugin_load_counter: HashMap::new(),
                    plugin_dir: PathBuf::new(),
                    changed_record: HashMap::new(),
                    monitor_lib_file: HashMap::new(),
                    plugin_infos: HashMap::new(),
                    lib_file_hash: HashMap::new(),
                    file_change_subscribers: Arc::new(Mutex::new(Vec::new())),
                    monitor_debounce,
                };
                let init_load_counter = 0;
                let lib_name_vec: Vec<String> = lib_name.as_ref().to_string()
                    .split(',')
                    .map(|item| item.trim().into())
                    .collect();

                let file_mapping =
                    watched_and_loaded_library_paths(&lib_dir, &lib_name_vec, init_load_counter, true);
                for (current_monitor_path, current_loading_path, plugin_name) in file_mapping {
                    if let Err(e) = hot_loading_manager
                        .loaded_plugins(
                            plugin_name,
                            current_monitor_path.display().to_string(),
                            current_loading_path.display().to_string(),
                            init_load_counter,
                        )
                        .into()
                    {
                        log::error!("{e}");
                        continue;
                    }
                }
                hot_loading_manager.plugin_dir = lib_dir.as_ref().to_path_buf();
                // let lib_loader = ::std::sync::Arc::new(::std::sync::Mutex::new(hot_loading_manager));
                // lib_loader.clone().try_lock().unwrap
                Ok(hot_loading_manager)
            }


            pub fn subscribe_to_file_changes(&mut self) -> mpsc::Receiver<(String, PluginLibEvent)> {
                log::trace!("subscribe to file change");
                let (tx, rx) = mpsc::channel();
                let mut subscribers = self.file_change_subscribers.lock().unwrap();
                subscribers.push(tx);
                self.monitor_plugin_dir(self.plugin_dir.clone(),self.file_change_subscribers.clone(), self.monitor_debounce
                .clone()
                .unwrap_or_else(|| Duration::from_millis(500))).unwrap();
                rx
            }

            /// Checks if the watched library has changed. If it has, reload it and return
            /// true. Otherwise return false.
            pub fn update(&mut self, lib_name: impl AsRef<str>) -> Result<bool, HotReloaderError> {
                if let Some(currentt_changed) = self.changed_record.get(lib_name.as_ref()) {
                    if !currentt_changed.load(Ordering::Acquire) {
                        return Ok(false);
                    }
                    currentt_changed.store(false, Ordering::Release);

                    self.reload(lib_name)?;
                }
                Ok(true)
            }

            /// Reload library `self.lib_file`.
            fn reload(&mut self, current_lib_name: impl AsRef<str>) -> Result<(), HotReloaderError> {
                let Self {
                    plugin_load_counter,
                    plugin_dir,
                    monitor_lib_file,
                    plugin_infos,
                    ..
                } = self;
                let current_monitor_lib_file = match monitor_lib_file.get(current_lib_name.as_ref()) {
                    Some(path_buf) => path_buf,
                    None => return Err(HotReloaderError::LibraryloadAccidentError(std::format!("reload lib `{}` failed, not found lib in monitor_lib_file object",current_lib_name.as_ref()).into())),
                };
                log::info!("reloading lib {current_monitor_lib_file:?}");

                // Close the loaded lib, copy the new lib to a file we can load, then load it.
                if let Some(current_monitor_lib_file) = monitor_lib_file.get(current_lib_name.as_ref()) {
                    if current_monitor_lib_file.exists() {
                        let current_load_counter = if let Some(current_load_counter) =
                        plugin_load_counter.get_mut(current_lib_name.as_ref())
                        {
                            *current_load_counter += 1;
                            *current_load_counter
                        } else {
                            let current_load_counter = 0;
                            current_load_counter
                        };

                        let lib_file_mapping = watched_and_loaded_library_paths(
                            plugin_dir.clone(),
                            &std::vec![current_lib_name.as_ref()],
                            current_load_counter,
                            false,
                        );
                        if let Some((current_watched_lib_file, current_loaded_lib_file, _)) =
                            lib_file_mapping.first()
                        {
                            log::trace!("copy {current_watched_lib_file:?} -> {current_loaded_lib_file:?}");
                            fs::copy(current_watched_lib_file, &current_loaded_lib_file)?;
                              // reload plugin lib
                              let load_result = (|| {
                                let header = lib_header_from_path(&current_loaded_lib_file)?;
                                header.init_root_module::<PluginObjectRef>()
                            })();
                            let plugin_id = PluginId {
                                named: current_lib_name.as_ref().into(),
                                path: current_loaded_lib_file.display().to_string().into(),
                                del_flog: false,
                            };
                            let plugin_ref = load_result?;
                            let plugin_source = match plugin_ref.new()(plugin_id.clone()) {
                                ROk(plugin_source) => plugin_source,
                                RErr(e) => {
                                    return Err(HotReloaderError::from(e));
                                }
                            };
                            if let Some(plugin_object) = self.plugin_mapping.remove(current_lib_name.as_ref()) {
                                plugin_object.lock().map_err(|e| HotReloaderError::LibraryloadAccidentError(RString::from(e.to_string())))?.close();
                                if let Some(current_loaded_lib_file) = plugin_infos.get(current_lib_name.as_ref()) {
                                    let path = PathBuf::from(current_loaded_lib_file.path.as_str());
                                    if path.exists() {
                                        let _ = fs::remove_file(path);
                                    }
                                }
                            }
                            plugin_load_counter.insert(current_lib_name.as_ref().into(), current_load_counter);
                            let current_lib_file_hash_value = hash_file(current_loaded_lib_file);
                            if let Some(current_lib_file_hash) =
                                self.lib_file_hash.get(current_lib_name.as_ref())
                            {
                                current_lib_file_hash.store(current_lib_file_hash_value, Ordering::Release);
                            } else {
                                let current_lib_file_hash =
                                    Arc::new(AtomicU32::new(current_lib_file_hash_value));
                                self.lib_file_hash
                                    .insert(current_lib_name.as_ref().into(), current_lib_file_hash);
                                let current_changed = Arc::new(AtomicBool::new(false));
                                self.changed_record
                                    .insert(current_lib_name.as_ref().into(), current_changed);
                            }
                            self.plugin_mapping.insert(
                                current_lib_name.as_ref().into(),
                                Arc::new(Mutex::new(plugin_source)),
                            );
                            self.plugin_infos.insert(
                                current_lib_name.as_ref().into(),
                                plugin_id,
                            );
                        }
                    } else {
                        log::warn!("trying to reload library but it does not exist");
                    }
                }

                Ok(())
            }

            fn monitor_reload(
                &self,
                current_lib_name: String,
                lib_file: impl AsRef<Path>,
                lib_file_hash: Arc<AtomicU32>,
                changed: Arc<AtomicBool>,
                file_change_subscribers: Arc<Mutex<Vec<mpsc::Sender<(String, PluginLibEvent)>>>>,
                debounce: Duration,
            ) -> Result<(), HotReloaderError> {
                let lib_file = lib_file.as_ref().to_path_buf();
                log::info!("start watching changes of file {}", lib_file.display());
                // File watcher thread. We watch `self.lib_file`, when it changes and we haven't
                // a pending change still waiting to be loaded, set `self.changed` to true. This
                // then gets picked up by `self.update`.
                thread::spawn(move || {
                    use DebouncedEvent::*;

                    let (tx, rx) = mpsc::channel();
                    let mut watcher = watcher(tx, debounce).unwrap();
                    watcher
                        .watch(&lib_file, RecursiveMode::NonRecursive)
                        .expect("watch lib file");

                    let signal_change = |event: PluginLibEvent| {
                        match event {
                            PluginLibEvent::Remove(_) => {},
                            _ => {
                                if hash_file(&lib_file) == lib_file_hash.load(Ordering::Acquire)
                                || changed.load(Ordering::Acquire)
                                {
                                    // file not changed
                                    return false;
                                }
                                log::debug!("{lib_file:?} changed",);
                                changed.store(true, Ordering::Release);
                            }
                        }
                        // inform subscribers
                        let subscribers = file_change_subscribers.lock().unwrap();
                        log::trace!(
                            "sending ChangedEvent::LibFileChanged to {} subscribers",
                            subscribers.len()
                        );
                        for tx in &*subscribers {
                            let _ = tx.send((current_lib_name.clone(),event.clone()));
                        }

                        true
                    };

                    loop {
                        let event = rx.recv();
                        log::trace!("file change event: {event:?}");
                        match event {
                            Ok(DebouncedEvent::Chmod(_) | DebouncedEvent::Write(_)) => {
                                let plugin_lib_event = PluginLibEvent::Other;
                                signal_change(plugin_lib_event);
                            }
                            Ok(DebouncedEvent::Remove(path)) => {
                                // just one hard link removed?
                                if !lib_file.exists() {
                                    log::debug!(
                                        "{} was removed, trying to watch it again...",
                                        lib_file.display()
                                    );
                                    let plugin_lib_event = PluginLibEvent::Remove(path.display().to_string());
                                    signal_change(plugin_lib_event);
                                    loop {
                                        if watcher
                                            .watch(&lib_file, RecursiveMode::NonRecursive)
                                            .is_ok()
                                        {
                                            log::info!("watching {lib_file:?} again after removal");
                                            let plugin_lib_event = PluginLibEvent::Other;
                                            signal_change(plugin_lib_event);
                                            break;
                                        }
                                        thread::sleep(Duration::from_millis(500));
                                    }
                                }
                            }
                            Ok(change) => {
                            }
                            Err(err) => {
                                log::error!("file watcher error, stopping reload loop: {err}");
                                break;
                            }
                        }
                    }
                });

                Ok(())
            }

            ///  Handle new connected devices
            fn monitor_plugin_dir(
                &self,
                lib_dir: impl AsRef<Path>,
                file_change_subscribers: Arc<Mutex<Vec<mpsc::Sender<(String, PluginLibEvent)>>>>,
                debounce: Duration,
            ) -> Result<(), HotReloaderError> {
                let lib_dir = lib_dir.as_ref().to_path_buf();
                log::info!("start watching changes of dir: {}", lib_dir.display());

                // File watcher thread. We watch `self.lib_file`, when it changes and we haven't
                // a pending change still waiting to be loaded, set `self.changed` to true. This
                // then gets picked up by `self.update`.
                thread::spawn(move || {
                    let (tx, rx) = mpsc::channel();
                    let mut watcher = watcher(tx, debounce).unwrap();
                    watcher
                        .watch(&lib_dir, RecursiveMode::NonRecursive)
                        .expect("watch lib dir");

                    let signal_change = |event_msg:PathBuf, event: PluginLibEvent| {
                        let lib_name = if let Some(lib_name) = get_lib_name_from_path(event_msg) {
                            lib_name
                        }else {
                            return false;
                        };
                        // inform subscribers
                        let subscribers = file_change_subscribers.lock().unwrap();
                        log::trace!(
                            "sending ChangedEvent::LibdirChanged to {} subscribers: {}",
                            subscribers.len(), lib_name
                        );
                        for tx in &*subscribers {
                            let _ = tx.send((lib_name.clone(), event.clone()));
                        }
                        true
                    };

                    loop {
                        let event = rx.recv();
                        log::trace!("plugin lib dir change event: {event:?}");
                        match event {
                            Ok(DebouncedEvent::Create(path)) => {
                                let plugin_lib_event = PluginLibEvent::Create(path.display().to_string());
                                signal_change(path, plugin_lib_event);
                            }
                            Ok(change) => {
                            }
                            Err(err) => {
                                log::error!("dir watcher error, stopping reload loop: {err}");
                                break;
                            }
                        }
                    }
                });

                Ok(())
            }

            /// Helper to log from the macro without requiring the user to have the log
            /// crate around
            #[doc(hidden)]
            pub fn log_info(what: impl std::fmt::Display) {
                log::info!("{}", what);
            }
        }

        impl PluginManager for HotLoadingManager {
            fn loaded_plugins(
                &mut self,
                plugin_name: String,
                monitor_path: String,
                loading_path: String,
                load_counter: usize,
            ) -> Result<PluginId, HotReloaderError> {
                if let Some(plugin_id)  = self.plugin_infos.get_mut(&plugin_name) {
                    plugin_id.del_flog = false;
                    return Ok(plugin_id.clone());
                } else {
                    let monitor_path = PathBuf::from(monitor_path.as_str());
                    let loading_path = PathBuf::from(loading_path.as_str());
                    let plugin_id = PluginId {
                        named: RString::from(plugin_name.clone()),
                        path: loading_path.display().to_string().into(),
                        del_flog: false,
                    };
                    let (current_lib_file_hash, current_plugin_source) = if monitor_path.exists() {
                        // We don't load the actual lib because this can get problems e.g. on Windows
                        // where a file lock would be held, preventing the lib from changing later.
                        log::debug!("copying {monitor_path:?} -> {loading_path:?}");
                        if let Err(e) = fs::copy(&monitor_path, &loading_path) {
                            return Err(HotReloaderError::from(e));
                        }
                        let load_result = (|| {
                            let header = lib_header_from_path(&loading_path)?;
                            header.init_root_module::<PluginObjectRef>()
                        })();
                        let plugin_ref = match load_result {
                            Ok(plugin_ref) => plugin_ref,
                            Err(e) => return Err(HotReloaderError::from(e)),
                        };
                        let lib_file_hash = hash_file(&loading_path);
                        let plugin_source = match plugin_ref.new()(plugin_id.clone()) {
                            ROk(plugin_source) => plugin_source,
                            RErr(e) => {
                                return Err(HotReloaderError::from(e));
                            }
                        };
                        (lib_file_hash, plugin_source)
                    } else {
                        log::debug!("library {monitor_path:?} does not yet exist");
                        return Err(HotReloaderError::LibraryNotLoaded);
                    };

                    let current_lib_file_hash = Arc::new(AtomicU32::new(current_lib_file_hash));
                    let current_changed = Arc::new(AtomicBool::new(false));

                    if let Err(e) = self.monitor_reload(
                        plugin_name.clone(),
                        monitor_path.clone(),
                        current_lib_file_hash.clone(),
                        current_changed.clone(),
                        self.file_change_subscribers.clone(),
                        self.monitor_debounce
                            .clone()
                            .unwrap_or_else(|| Duration::from_millis(500)),
                    ) {
                        return Err(e);
                    }


                    self.plugin_infos
                        .insert(plugin_name.clone(), plugin_id.clone());
                    self.plugin_mapping.insert(
                        plugin_name.clone(),
                        Arc::new(Mutex::new(current_plugin_source)),
                    );

                    self.changed_record
                        .insert(plugin_name.clone(), current_changed);
                    self.lib_file_hash
                        .insert(plugin_name.clone(), current_lib_file_hash);
                    self.plugin_load_counter
                        .insert(plugin_name.clone(), load_counter);
                    self.monitor_lib_file
                        .insert(plugin_name, monitor_path);

                    return Ok(plugin_id);
                }
                //else {

                    // }

                    // RErr(HotReloaderError::LibraryloadAccidentError(
                    //     std::format!("Plugin `{}` already exists", plugin_name).into(),
                    // ))
            }


            fn unloaded_plugins(&mut self, plugin_name: String) -> Result<PluginId, HotReloaderError> {
                // if self.plugin_mapping.contains_key(plugin_name.as_str()) {
                //     let plugin_object = self.plugin_mapping.remove(plugin_name.clone().into());
                //     if let RSome(plugin_object) = plugin_object {
                //         plugin_object.close();
                //     }
                //     self.changed_record.remove(plugin_name.clone().into());
                //     self.lib_file_hash.remove(plugin_name.clone().into());
                //     self.plugin_load_counter.remove(plugin_name.clone().into());
                //     self.monitor_lib_file.remove(plugin_name.clone().into());

                //     let plugin_id = self.plugin_infos.remove(plugin_name.clone().into());
                //     if let RSome(plugin_id) = plugin_id {
                //         log::info!("unloaded plugin sucessful: {:?}", plugin_id);
                //         return ROk(plugin_id);
                //     }
                // }
                if let Some(plugin_id) = self.plugin_infos.get_mut(&plugin_name) {
                    plugin_id.del_flog = true;
                    return Ok(plugin_id.clone());
                }
                Err(HotReloaderError::LibraryloadAccidentError(
                    std::format!("Plugin `{}` not found", plugin_name).into(),
                ))
            }

            fn get_plugins(&self) -> HashMap<String, PluginId> {
                self.plugin_infos.clone()
            }
        }
    };
}

// use abi_stable::std_types::RString;
// use std::{
//     fs,
//     path::{Path, PathBuf},
//     sync::{
//         atomic::{AtomicBool, AtomicU32, Ordering},
//         mpsc, Arc, Mutex,
//     },
//     thread,
//     time::Duration,
// };
// use notify::DebouncedEvent;
// use common_define::{error::HotReloaderError, utils::{PluginLibEvent, path_handler::{get_lib_name_from_path, watched_and_loaded_library_paths}}};
// fn monitor_reload(
//     current_lib_name: RString,
//     lib_file: impl AsRef<Path>,
//     lib_file_hash: Arc<AtomicU32>,
//     changed: Arc<AtomicBool>,
//     file_change_subscribers: Arc<Mutex<Vec<mpsc::Sender<(RString, PluginLibEvent)>>>>,
//     debounce: Duration,
// ) -> Result<(), HotReloaderError> {
//     let lib_file = lib_file.as_ref().to_path_buf();
//     log::info!("start watching changes of file {}", lib_file.display());

//     // File watcher thread. We watch `self.lib_file`, when it changes and we haven't
//     // a pending change still waiting to be loaded, set `self.changed` to true. This
//     // then gets picked up by `self.update`.

//     thread::spawn(move || {
//         use notify::DebouncedEvent::*;
//         use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
//         use common_define::utils::path_handler::hash_file;
//         let (tx, rx) = mpsc::channel();
//         let mut watcher = watcher(tx, debounce).unwrap();
//         watcher
//             .watch(&lib_file, RecursiveMode::NonRecursive)
//             .expect("watch lib file");

//         let signal_change = |event: PluginLibEvent| {

//             if hash_file(&lib_file) == lib_file_hash.load(Ordering::Acquire)
//                 || changed.load(Ordering::Acquire)
//             {
//                 // file not changed
//                 return false;
//             }

//             log::debug!("{lib_file:?} changed",);

//             changed.store(true, Ordering::Release);

//             // inform subscribers
//             let subscribers = file_change_subscribers.lock().unwrap();
//             log::trace!(
//                 "sending ChangedEvent::LibFileChanged to {} subscribers",
//                 subscribers.len()
//             );
//             for tx in &*subscribers {
//                 let _ = tx.send((current_lib_name.clone(), event.clone()));
//             }

//             true
//         };

//         loop {
//             let event = rx.recv();
//             log::trace!("file change event: {event:?}");
//             match event {
//                 Ok(Chmod(path) | Write(path)) => {
//                     let plugin_lib_event = PluginLibEvent::Other;
//                     signal_change(plugin_lib_event);
//                 }
//                 Ok( Create(path)) => {
//                     let plugin_lib_event = PluginLibEvent::Create(RString::from(path.display().to_string()));
//                     signal_change(plugin_lib_event);
//                 }
//                 Ok(Remove(path)) => {
//                     // just one hard link removed?
//                     if !lib_file.exists() {
//                         log::debug!(
//                             "{} was removed, trying to watch it again...",
//                             lib_file.display()
//                         );
//                     }
//                     loop {
//                         if watcher
//                             .watch(&lib_file, RecursiveMode::NonRecursive)
//                             .is_ok()
//                         {
//                             log::info!("watching {lib_file:?} again after removal");
//                             signal_change();
//                             break;
//                         }
//                         thread::sleep(Duration::from_millis(500));
//                     }
//                 }
//                 Ok(change) => {
//                 }
//                 Err(err) => {
//                     log::error!("file watcher error, stopping reload loop: {err}");
//                     break;
//                 }
//             }
//         }
//     });

//     Ok(())
// }

// ///  Handle new connected devices
// fn monitor_plugin_dir(
//     lib_dir: impl AsRef<Path>,
//     file_change_subscribers: Arc<Mutex<Vec<mpsc::Sender<(RString, PluginLibEvent)>>>>,
//     debounce: Duration,
// ) -> Result<(), HotReloaderError> {
//     let lib_dir = lib_dir.as_ref().to_path_buf();
//     log::info!("start watching changes of dir: {}", lib_dir.display());

//     // File watcher thread. We watch `self.lib_file`, when it changes and we haven't
//     // a pending change still waiting to be loaded, set `self.changed` to true. This
//     // then gets picked up by `self.update`.

//     thread::spawn(move || {
//         use notify::DebouncedEvent::*;
//         use notify::{watcher, RecursiveMode, Watcher};
//         use common_define::utils::path_handler::hash_file;
//         let (tx, rx) = mpsc::channel();
//         let mut watcher = watcher(tx, debounce).unwrap();
//         watcher
//             .watch(&lib_dir, RecursiveMode::NonRecursive)
//             .expect("watch lib file");

//         let signal_change = |event_msg:PathBuf, event: PluginLibEvent| {
//             let lib_name = if let Some(lib_name) = get_lib_name_from_path(event_msg) {
//                 lib_name
//             }else {
//                 return false;
//             };
//             // inform subscribers
//             let subscribers = file_change_subscribers.lock().unwrap();
//             log::trace!(
//                 "sending ChangedEvent::LibFileChanged to {} subscribers",
//                 subscribers.len()
//             );
//             for tx in &*subscribers {
//                 let _ = tx.send((RString::from(lib_name), event.clone()));
//             }
//             true
//         };

//         loop {
//             let event = rx.recv();
//             log::trace!("plugin lib dir change event: {event:?}");
//             match event {
//                 Ok(Create(path)) => {
//                     let plugin_lib_event = PluginLibEvent::Create(RString::from(path.display().to_string()));
//                     signal_change(path, plugin_lib_event);
//                 }
//                 Ok(change) => {
//                 }
//                 Err(err) => {
//                     log::error!("file watcher error, stopping reload loop: {err}");
//                     break;
//                 }
//             }
//         }
//     });

//     Ok(())
// }
