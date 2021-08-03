use {
    goods_treasury_import::{Importer, Registry},
    std::path::Path,
    uuid::Uuid,
};

const BUFFER_LEN: usize = 1024;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ImporterOpaque {
    _byte: u8,
}

#[repr(C)]
pub struct ImporterFFI {
    data: *const ImporterOpaque,
    name: unsafe fn(*const ImporterOpaque, *mut u8, usize) -> usize,
    source: unsafe fn(*const ImporterOpaque, *mut u8, usize) -> usize,
    native: unsafe fn(*const ImporterOpaque, *mut u8, usize) -> usize,

    #[cfg(any(unix, target_os = "wasi"))]
    import: unsafe fn(
        *const ImporterOpaque,
        *const u8,
        usize,
        *const u8,
        usize,
        *mut u8,
        usize,
    ) -> isize,

    #[cfg(windows)]
    import: unsafe fn(
        *const ImporterOpaque,
        *const u16,
        usize,
        *const u16,
        usize,
        *mut u8,
        usize,
    ) -> isize,
}

impl ImporterFFI {
    pub fn new<I>(importer: &'static I) -> Self
    where
        I: Importer,
    {
        ImporterFFI {
            data: importer as *const I as *const ImporterOpaque,
            name: |data, buf, len| unsafe {
                let name = (*(data as *mut I)).name();
                let buf = std::slice::from_raw_parts_mut(buf, len);
                buf[..len.min(name.len())].copy_from_slice(&name.as_bytes());
                name.len()
            },
            source: |data, buf, len| unsafe {
                let source = (*(data as *mut I)).source();
                let buf = std::slice::from_raw_parts_mut(buf, len);
                buf[..len.min(source.len())].copy_from_slice(&source.as_bytes());
                source.len()
            },
            native: |data, buf, len| unsafe {
                let native = (*(data as *mut I)).native();
                let buf = std::slice::from_raw_parts_mut(buf, len);
                buf[..len.min(native.len())].copy_from_slice(&native.as_bytes());
                native.len()
            },
            import: |data, source_ptr, source_len, native_ptr, native_len, error_ptr, error_len| unsafe {
                #[cfg(any(unix, target_os = "wasi"))]
                let (source, native) = {
                    use std::ffi::OsStr;

                    #[cfg(unix)]
                    use std::os::unix::ffi::OsStrExt;
                    #[cfg(target_os = "wasi")]
                    use std::os::wasi::ffi::OsStrExt;

                    let source =
                        OsStr::from_bytes(std::slice::from_raw_parts(source_ptr, source_len));

                    let native =
                        OsStr::from_bytes(std::slice::from_raw_parts(native_ptr, native_len));

                    (source, native)
                };

                #[cfg(windows)]
                let (source, native) = {
                    use std::{ffi::OsString, os::windows::ffi::OsStringExt};

                    let source =
                        OsString::from_wide(std::slice::from_raw_parts(source_ptr, source_len));

                    let native =
                        OsString::from_wide(std::slice::from_raw_parts(native_ptr, native_len));

                    (source, native)
                };

                match (*(data as *const I)).import(
                    source.as_ref(),
                    native.as_ref(),
                    &mut RegistryFFI,
                ) {
                    Ok(()) => 0,
                    Err(err) => {
                        use std::io::{Cursor, Write as _};
                        let error_slice = std::slice::from_raw_parts_mut(error_ptr, error_len);
                        let mut error_write = Cursor::new(error_slice);

                        let _ = write!(error_write, "{:#}", err);
                        -(error_write.position() as isize)
                    }
                }
            },
        }
    }
}

struct RegistryFFI;

impl Registry for RegistryFFI {
    fn store(
        &mut self,
        source: &Path,
        source_format: &str,
        native_format: &str,
        tags: &[&str],
    ) -> eyre::Result<Uuid> {
        use std::ffi::OsStr;

        #[cfg(unix)]
        use std::os::unix::ffi::OsStrExt;
        #[cfg(target_os = "wasi")]
        use std::os::wasi::ffi::OsStrExt;
        #[cfg(windows)]
        use std::os::windows::ffi::OsStrExt;

        let source: &OsStr = source.as_ref();

        #[cfg(any(unix, target_os = "wasi"))]
        let source = source.as_bytes();

        #[cfg(windows)]
        let source = source.encode_wide().collect::<Vec<u16>>();

        #[cfg(windows)]
        let source = String::from_utf16(&source[..]).unwrap();

        let mut result_array = [0u8; BUFFER_LEN];

        let tag_count = tags.len();
        let tag_ptrs = tags.iter().map(|t| str::as_ptr(t)).collect::<Vec<_>>();
        let tag_lens = tags.iter().map(|t| str::len(t)).collect::<Vec<_>>();

        let result = unsafe {
            treasury_registry_store(
                source.as_ptr(),
                source.len(),
                source_format.as_ptr(),
                source_format.len(),
                native_format.as_ptr(),
                native_format.len(),
                tag_ptrs.as_ptr(),
                tag_lens.as_ptr(),
                tag_count,
                result_array.as_mut_ptr(),
                BUFFER_LEN,
            )
        };

        if result < 0 {
            let len = result.abs() as usize;
            let error = std::str::from_utf8(&result_array[..len.min(BUFFER_LEN)]).unwrap();
            Err(eyre::eyre!("{}", error))
        } else {
            // success
            debug_assert_eq!(result, 16);
            let uuid = Uuid::from_slice(&result_array[..16]).unwrap();
            Ok(uuid)
        }
    }

    fn fetch(&mut self, asset: &Uuid) -> eyre::Result<Box<Path>> {
        let asset = asset.as_bytes();

        let mut path_array = [0; BUFFER_LEN];
        let mut error_array = [0; BUFFER_LEN];

        let result = unsafe {
            treasury_registry_fetch(
                asset.as_ptr(),
                path_array.as_mut_ptr(),
                BUFFER_LEN,
                error_array.as_mut_ptr(),
                BUFFER_LEN,
            )
        };

        if result < 0 {
            let len = result.abs() as usize;
            let error = std::str::from_utf8(&error_array[..len.min(BUFFER_LEN)]).unwrap();
            Err(eyre::eyre!("{}", error))
        } else {
            let len = result as usize;
            debug_assert!(len <= BUFFER_LEN);

            #[cfg(any(unix, target_os = "wasi"))]
            {
                use std::ffi::OsStr;

                #[cfg(unix)]
                use std::os::unix::ffi::OsStrExt;
                #[cfg(target_os = "wasi")]
                use std::os::wasi::ffi::OsStrExt;

                let path = OsStr::from_bytes(&path_array[..len.min(BUFFER_LEN)]);

                Ok(Path::new(path).into())
            }

            #[cfg(windows)]
            {
                use std::path::PathBuf;
                let path = std::str::from_utf8(&path_array[..len.min(BUFFER_LEN)]).unwrap();
                Ok(PathBuf::from(path).into_boxed_path())
            }
        }
    }
}

extern "C" {
    fn treasury_registry_store(
        source_ptr: *const u8,
        source_len: usize,
        source_format_ptr: *const u8,
        source_format_len: usize,
        native_format_ptr: *const u8,
        native_format_len: usize,
        tag_ptrs: *const *const u8,
        tag_lens: *const usize,
        tag_count: usize,
        result_ptr: *mut u8,
        result_len: usize,
    ) -> isize;

    fn treasury_registry_fetch(
        uuid: *const u8,
        path_ptr: *mut u8,
        path_len: usize,
        error_ptr: *mut u8,
        error_len: usize,
    ) -> isize;
}

#[doc(hidden)]
#[macro_export]
macro_rules! count_tt {
    () => { 0 };
    ($head:tt $(, $tail:tt)*) => { 1 + $crate::count_tt!($($tail),*) };
}

/// Generates FFI-safe function to enumerate importers
#[macro_export]
macro_rules! generate_imports_and_exports {
    ($($importer:expr),* $(,)?) => {

        #[no_mangle]
        pub unsafe fn treasury_importer_enumerate_importers(importers: *mut $crate::ffi::ImporterFFI, count: usize) -> usize {
            const COUNT: usize = $crate::count_tt!($(($importer)),*);

            if count < COUNT {
                COUNT
            } else {
                let mut ptr = importers;

                $(
                    ::std::ptr::write(ptr, $crate::ffi::ImporterFFI::new($importer));
                    ptr = ptr.add(1);
                )*

                COUNT
            }
        }
    };
}

#[cfg(target_os = "wasi")]
#[no_mangle]
pub unsafe fn treasury_importer_name_source_native_trampoline(
    function: unsafe fn(*const ImporterOpaque, *mut u8, u32) -> u32,
    ptr: *const ImporterOpaque,
    result_ptr: *mut u8,
    result_len: u32,
) -> u32 {
    function(ptr, result_ptr, result_len)
}

#[cfg(target_os = "wasi")]
#[no_mangle]
pub unsafe fn treasury_importer_import_trampoline(
    function: unsafe fn(*const ImporterOpaque, *const u8, u32, *const u8, u32, *mut u8, u32) -> i32,
    ptr: *const ImporterOpaque,
    source_path_ptr: *const u8,
    source_path_len: u32,
    native_path_ptr: *const u8,
    native_path_len: u32,
    error_ptr: *mut u8,
    error_len: u32,
) -> i32 {
    function(
        ptr,
        source_path_ptr,
        source_path_len,
        native_path_ptr,
        native_path_len,
        error_ptr,
        error_len,
    )
}

/// # Safety
///
/// This function is export of standard function `alloc::alloc::alloc`.
/// Same safety principles applies.
#[no_mangle]
pub unsafe fn treasury_importer_alloc(size: usize, align: usize) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size, align).unwrap();
    std::alloc::alloc(layout)
}

/// # Safety
///
/// This function is export of standard function `alloc::alloc::dealloc`.
/// Same safety principles applies.
#[no_mangle]
pub unsafe fn treasury_importer_dealloc(ptr: *mut u8, size: usize, align: usize) {
    let layout = std::alloc::Layout::from_size_align(size, align).unwrap();
    std::alloc::dealloc(ptr, layout);
}
