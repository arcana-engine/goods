use {
    crate::treasury::Registry,
    std::{
        collections::hash_map::HashMap,
        path::{Path, PathBuf},
        sync::{Arc, Mutex, MutexGuard, Weak},
    },
    uuid::Uuid,
    wasmer::{
        Array, Function, Instance, LazyInit, Memory, Module, NativeFunc, Store, WasmPtr, WasmerEnv,
    },
    wasmer_wasi::{WasiEnv, WasiState},
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ImporterOpaque {
    _byte: u8,
}

const WASM_IMPORTERS_INITIAL_COUNT: u32 = 64;
const ERROR_BUFFER_LEN: u32 = 2048;

#[cfg(any(unix, target_os = "wasi"))]
const OS_CHAR_SIZE: u32 = 1;
#[cfg(windows)]
const OS_CHAR_SIZE: u32 = 2;

pub(crate) struct Importers {
    map: HashMap<Box<str>, HashMap<Box<str>, Arc<WasmImporter>>>,
    store: Store,
    wasi: WasiEnv,
}

impl Importers {
    pub fn new(treasury_path: &Path) -> Self {
        let store = Store::default();

        let cd = std::env::current_dir().unwrap();

        let wasi = WasiState::new("treasury")
            .preopen(|p| p.directory(&cd).read(true))
            .unwrap()
            .preopen(|p| {
                p.directory(treasury_path)
                    .read(true)
                    .write(true)
                    .create(true)
            })
            .unwrap()
            .finalize()
            .unwrap();

        Importers {
            wasi,
            map: HashMap::new(),
            store,
        }
    }

    pub fn get_importer(&self, source: &str, native: &str) -> Option<Arc<WasmImporter>> {
        self.map.get(source)?.get(native).cloned()
    }

    pub fn load_importers_dir(
        &mut self,
        dir_path: &Path,
        registry: &Arc<Mutex<Registry>>,
    ) -> std::io::Result<()> {
        let dir = std::fs::read_dir(dir_path)?;

        for e in dir {
            let e = e?;
            let path = PathBuf::from(e.file_name());
            let is_wasm_module = path.extension().map_or(false, |e| e == "wasm");

            if is_wasm_module {
                let wasm_path = dir_path.join(path);
                if let Err(err) = self.load_importers(&wasm_path, registry) {
                    tracing::warn!(
                        "Could not load importers from '{}'. {:#}",
                        wasm_path.display(),
                        err
                    );
                }
            }
        }

        Ok(())
    }

    fn load_importers(
        &mut self,
        wasm_path: &Path,
        registry: &Arc<Mutex<Registry>>,
    ) -> eyre::Result<()> {
        tracing::trace!("Load importers from: {}", wasm_path.display());

        let bytes = std::fs::read(wasm_path)?;

        if !wasmer::is_wasm(&bytes) {
            return Err(eyre::eyre!("Not a WASM module"));
        }

        let module = Module::new(&self.store, &bytes)?;

        let mut imports = self.wasi.import_object(&module)?;

        let env = ImporterEnv {
            memory: LazyInit::new(),
            registry: Arc::downgrade(registry),
        };

        imports.register("env", wasmer::import_namespace! {{
            "treasury_registry_store" => Function::new_native_with_env(&self.store, env.clone(), treasury_registry_store),
            "treasury_registry_fetch" => Function::new_native_with_env(&self.store, env, treasury_registry_fetch),
        }});

        let instance = Instance::new(&module, &imports)?;

        let memory = instance.exports.get_memory("memory")?;

        let alloc = instance
            .exports
            .get_native_function::<(u32, u32), WasmPtr<u8, Array>>("treasury_importer_alloc")?;

        let dealloc = instance
            .exports
            .get_native_function::<(WasmPtr<u8, Array>, u32, u32), ()>(
                "treasury_importer_dealloc",
            )?;

        let name_source_native_trampoline = instance.exports.get_native_function::<(
            WasmPtr<fn()>,
            WasmPtr<ImporterOpaque>,
            WasmPtr<u8, Array>,
            u32,
        ), u32>(
            "treasury_importer_name_source_native_trampoline",
        )?;

        let importer_import_trampoline =
            instance.exports.get_native_function::<(
                WasmPtr<fn()>,
                WasmPtr<ImporterOpaque>,
                WasmPtr<u8, Array>,
                u32,
                WasmPtr<u8, Array>,
                u32,
                WasmPtr<u8, Array>,
                u32,
            ), i32>("treasury_importer_import_trampoline")?;

        let enumerate_importers = instance
            .exports
            .get_native_function::<(WasmPtr<WasmImporterFFI, Array>, u32), u32>(
                "treasury_importer_enumerate_importers",
            )?;

        let mut allocated_size =
            std::mem::size_of::<WasmImporterFFI>() as u32 * WASM_IMPORTERS_INITIAL_COUNT;

        let mut ptr = alloc.call(allocated_size, 4)?;
        let mut importers_ptr = WasmPtr::<WasmImporterFFI, Array>::new(ptr.offset());

        let count = enumerate_importers.call(importers_ptr, WASM_IMPORTERS_INITIAL_COUNT)?;

        if count > WASM_IMPORTERS_INITIAL_COUNT {
            dealloc.call(ptr, allocated_size, 4)?;
            allocated_size = count * std::mem::size_of::<WasmImporterFFI>() as u32;
            ptr = alloc.call(allocated_size, 4)?;
            importers_ptr = WasmPtr::<WasmImporterFFI, Array>::new(ptr.offset());

            match enumerate_importers.call(importers_ptr, count)? {
                new_count if new_count == count => {
                    // success
                }
                _ => {
                    // failure
                    eyre::eyre!("Failed to enumerate importers from WASM module");
                }
            }
        }

        let state = Arc::new(WasmState {
            alloc,
            dealloc,
            name_source_native_trampoline,
            importer_import_trampoline,
            memory: memory.clone(),
        });

        let importers_ptr_u32 = WasmPtr::<u32, Array>::new(importers_ptr.offset());

        let ptrs = importers_ptr_u32.deref(memory, 0, count * 5).unwrap();

        for ptrs in ptrs.chunks_exact(5) {
            let ffi = match ptrs {
                [data, name, source, native, import] => WasmImporterFFI {
                    data: WasmPtr::new(data.get()),
                    name: WasmPtr::new(name.get()),
                    source: WasmPtr::new(source.get()),
                    native: WasmPtr::new(native.get()),
                    import: WasmPtr::new(import.get()),
                },
                _ => unreachable!(),
            };

            let importer = WasmImporter::new(ffi, state.clone());
            tracing::info!(
                "Importer '{}' from '{}' to '{}' loaded",
                importer.name(),
                importer.source(),
                importer.native()
            );

            self.map
                .entry(importer.source().into())
                .or_default()
                .entry(importer.native().into())
                .or_insert_with(|| Arc::new(importer));
        }

        state.dealloc.call(ptr, allocated_size, 4)?;

        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WasmImporterFFI {
    data: WasmPtr<ImporterOpaque>,
    name: WasmPtr<fn()>,
    source: WasmPtr<fn()>,
    native: WasmPtr<fn()>,
    import: WasmPtr<fn()>,
}

type NameSourceNativeFunctionArgs = (
    WasmPtr<fn()>,
    WasmPtr<ImporterOpaque>,
    WasmPtr<u8, Array>,
    u32,
);

type ImportFunctionArgs = (
    WasmPtr<fn()>,
    WasmPtr<ImporterOpaque>,
    WasmOsStrPtr,
    u32,
    WasmOsStrPtr,
    u32,
    WasmPtr<u8, Array>,
    u32,
);

struct WasmState {
    name_source_native_trampoline: NativeFunc<NameSourceNativeFunctionArgs, u32>,
    importer_import_trampoline: NativeFunc<ImportFunctionArgs, i32>,
    alloc: NativeFunc<(u32, u32), WasmPtr<u8, Array>>,
    dealloc: NativeFunc<(WasmPtr<u8, Array>, u32, u32)>,

    memory: Memory,
}

pub struct WasmImporter {
    ffi: WasmImporterFFI,
    state: Arc<WasmState>,
    name: String,
    source: String,
    native: String,
}

impl WasmImporter {
    fn new(ffi: WasmImporterFFI, state: Arc<WasmState>) -> Self {
        const STRING_CAP: u32 = 256;
        let wasm_ptr = state.alloc.call(STRING_CAP, 1).unwrap();

        let len = state
            .name_source_native_trampoline
            .call(ffi.name, ffi.data, wasm_ptr, STRING_CAP)
            .unwrap();

        let name = wasm_ptr.get_utf8_string(&state.memory, len).unwrap();

        let len = state
            .name_source_native_trampoline
            .call(ffi.source, ffi.data, wasm_ptr, STRING_CAP)
            .unwrap();

        let source = wasm_ptr.get_utf8_string(&state.memory, len).unwrap();

        let len = state
            .name_source_native_trampoline
            .call(ffi.native, ffi.data, wasm_ptr, STRING_CAP)
            .unwrap();

        let native = wasm_ptr.get_utf8_string(&state.memory, len).unwrap();

        state.dealloc.call(wasm_ptr, STRING_CAP, 1).unwrap();

        WasmImporter {
            ffi,
            state,
            name,
            source,
            native,
        }
    }
}

impl WasmImporter {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn native(&self) -> &str {
        &self.native
    }

    pub(crate) fn import(
        &self,
        source_path: &Path,
        native_path: &Path,
        registry: MutexGuard<'_, Registry>,
    ) -> eyre::Result<()> {
        drop(registry);

        #[cfg(unix)]
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        #[cfg(target_os = "wasi")]
        use std::{ffi::OsStr, os::wasi::ffi::OsStrExt};
        #[cfg(windows)]
        use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

        let source_path = OsStr::new(source_path);
        let native_path = OsStr::new(native_path);

        #[cfg(any(unix, target_os = "wasi"))]
        let source_path = source_path.as_bytes();

        #[cfg(any(unix, target_os = "wasi"))]
        let native_path = native_path.as_bytes();

        #[cfg(windows)]
        let source_path = source_path.encode_wide().collect::<Vec<_>>();
        #[cfg(windows)]
        let source_path = &source_path[..];

        #[cfg(windows)]
        let native_path = native_path.encode_wide().collect::<Vec<_>>();
        #[cfg(windows)]
        let native_path = &native_path[..];

        let size = OS_CHAR_SIZE * source_path.len() as u32
            + OS_CHAR_SIZE * native_path.len() as u32
            + ERROR_BUFFER_LEN;

        let ptr = self.state.alloc.call(size, OS_CHAR_SIZE).unwrap();

        let source_ptr = WasmOsStrPtr::new(ptr.offset());
        let native_ptr = WasmOsStrPtr::new(ptr.offset() + OS_CHAR_SIZE * source_path.len() as u32);

        let error_ptr = WasmPtr::<u8, Array>::new(
            ptr.offset()
                + OS_CHAR_SIZE * source_path.len() as u32
                + OS_CHAR_SIZE * native_path.len() as u32,
        );

        let slice = source_ptr
            .deref(&self.state.memory, 0, source_path.len() as u32)
            .unwrap();
        slice
            .iter()
            .zip(source_path)
            .for_each(|(cell, c)| cell.set(*c));

        let slice = native_ptr
            .deref(&self.state.memory, 0, native_path.len() as u32)
            .unwrap();

        slice
            .iter()
            .zip(native_path)
            .for_each(|(cell, c)| cell.set(*c));

        let result = self.state.importer_import_trampoline.call(
            self.ffi.import,
            self.ffi.data,
            source_ptr,
            source_path.len() as u32,
            native_ptr,
            native_path.len() as u32,
            error_ptr,
            ERROR_BUFFER_LEN,
        )?;

        if result < 0 {
            let len = result.abs() as u32;
            let error = error_ptr.get_utf8_string(&self.state.memory, len).unwrap();

            self.state.dealloc.call(ptr, size, OS_CHAR_SIZE)?;
            Err(eyre::eyre!("Importer error: '{}'", error))
        } else {
            self.state.dealloc.call(ptr, size, OS_CHAR_SIZE)?;
            debug_assert_eq!(result, 0);
            Ok(())
        }
    }
}

#[cfg(any(unix, target_os = "wasi"))]
type WasmOsStrPtr = WasmPtr<u8, Array>;

#[cfg(windows)]
type WasmStrPtr = WasmPtr<u16, Array>;

#[allow(clippy::too_many_arguments)]
fn treasury_registry_store(
    env: &ImporterEnv,
    source_ptr: WasmOsStrPtr,
    source_len: u32,
    source_format_ptr: WasmPtr<u8, Array>,
    source_format_len: u32,
    native_format_ptr: WasmPtr<u8, Array>,
    native_format_len: u32,
    tag_ptrs: WasmPtr<WasmPtr<u8, Array>, Array>,
    tag_lens: WasmPtr<u32, Array>,
    tag_count: u32,
    result_ptr: WasmPtr<u8, Array>,
    result_len: u32,
) -> i32 {
    #[cfg(unix)]
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    #[cfg(windows)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    #[cfg(target_os = "wasi")]
    use std::{ffi::Ostr, os::wasi::ffi::OsStrExt};

    let memory = env.memory_ref().unwrap();

    let source = source_ptr.deref(memory, 0, source_len).unwrap();

    let source = source.iter().map(std::cell::Cell::get).collect::<Vec<_>>();

    #[cfg(any(unix, target_os = "wasi"))]
    let source = OsStr::from_bytes(&source);

    #[cfg(windows)]
    let source = OsString::from_wide(&source);

    #[cfg(windows)]
    let source = &source;

    let source_format = source_format_ptr
        .deref(memory, 0, source_format_len)
        .unwrap();

    let source_format = source_format
        .iter()
        .map(std::cell::Cell::get)
        .collect::<Vec<_>>();

    let source_format = std::str::from_utf8(&source_format).unwrap();

    let native_format = native_format_ptr
        .deref(memory, 0, native_format_len)
        .unwrap();

    let native_format = native_format
        .iter()
        .map(std::cell::Cell::get)
        .collect::<Vec<_>>();

    let native_format = std::str::from_utf8(&native_format).unwrap();

    let tag_ptrs = tag_ptrs.deref(memory, 0, tag_count).unwrap();
    let tag_lens = tag_lens.deref(memory, 0, tag_count).unwrap();

    let tags = tag_ptrs
        .iter()
        .zip(tag_lens)
        .map(|(ptr, len)| ptr.get().get_utf8_string(memory, len.get()).unwrap())
        .collect::<Vec<_>>();

    let result = Registry::store(
        &env.registry.upgrade().unwrap(),
        Path::new(source),
        source_format,
        native_format,
        &tags,
    );

    match result {
        Ok(uuid) if result_len >= 16 => {
            let result = result_ptr.deref(memory, 0, 16).unwrap();
            result
                .iter()
                .zip(uuid.as_bytes())
                .for_each(|(cell, byte)| cell.set(*byte));
            16
        }
        Ok(_) => {
            tracing::error!(
                "Importer provided to short result buffer. At least 16 bytes long buffer is required for UUID on successful sub-import"
            );

            let error = b"Too short";
            let len = result_len.min(error.len() as u32);

            let result = result_ptr.deref(memory, 0, len).unwrap();

            result
                .iter()
                .zip(error)
                .for_each(|(cell, byte)| cell.set(*byte));

            -(len as i32)
        }
        Err(err) => {
            tracing::error!("Sub-import failed with. {:#}", err);

            let error = format!("{:#}", err);
            let len = result_len.min(error.len() as u32);

            let result = result_ptr.deref(memory, 0, len).unwrap();

            result
                .iter()
                .zip(error.as_bytes())
                .for_each(|(cell, byte)| cell.set(*byte));

            -(len as i32)
        }
    }
}

fn treasury_registry_fetch(
    env: &ImporterEnv,
    uuid: WasmPtr<u8, Array>,
    path_ptr: WasmOsStrPtr,
    path_len: u32,
    error_ptr: WasmPtr<u8, Array>,
    error_len: u32,
) -> i32 {
    #[cfg(unix)]
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    #[cfg(windows)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    #[cfg(target_os = "wasi")]
    use std::{ffi::Ostr, os::wasi::ffi::OsStrExt};

    let uuid = uuid.deref(env.memory_ref().unwrap(), 0, 16).unwrap();
    let mut bytes = [0u8; 16];
    bytes
        .iter_mut()
        .zip(uuid)
        .for_each(|(byte, cell)| *byte = cell.get());

    let uuid = Uuid::from_bytes(bytes);

    let result = Registry::fetch(&env.registry.upgrade().unwrap(), &uuid, 0);

    match result {
        Ok(None) => unreachable!(),
        Ok(Some(info)) => {
            let native_path = info.native_path;
            let native_path = OsStr::new(&*native_path);

            if path_len < native_path.len() as u32 {
                tracing::error!(
                    "Importer provided to short result buffer. At least {} chars long buffer is required.\nImporter should allocate buffer large enough to fit any sensible path length", native_path.len() as u32,
                );

                let err = b"Path buffer is too small";
                let len = error_len.min(err.len() as u32);
                let error = error_ptr.deref(env.memory_ref().unwrap(), 0, len).unwrap();

                error
                    .iter()
                    .zip(&err[..])
                    .for_each(|(cell, byte)| cell.set(*byte));

                -(len as i32)
            } else {
                let len = native_path.len() as u32;

                let path = path_ptr.deref(env.memory_ref().unwrap(), 0, len).unwrap();

                #[cfg(any(unix, target_os = "wasi"))]
                path.iter()
                    .zip(native_path.as_bytes())
                    .for_each(|(cell, c)| cell.set(*c));

                #[cfg(windows)]
                path.iter()
                    .zip(native_path.encode_wide())
                    .for_each(|(cell, c)| cell.set(*c));

                len as i32
            }
        }
        Err(err) => {
            let err = format!("{:#}", err);
            let err = err.as_bytes();
            let len = error_len.min(err.len() as u32);
            let error = error_ptr.deref(env.memory_ref().unwrap(), 0, len).unwrap();

            error
                .iter()
                .zip(err)
                .for_each(|(cell, byte)| cell.set(*byte));

            -(len as i32)
        }
    }
}

#[derive(Clone, WasmerEnv)]
pub struct ImporterEnv {
    #[wasmer(export)]
    memory: LazyInit<Memory>,

    registry: Weak<Mutex<Registry>>,
}
