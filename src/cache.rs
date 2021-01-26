use std::convert::TryInto;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::str::from_utf8;

use cosmwasm_vm::{features_from_csv, Cache, CacheOptions, Checksum, Size};

use crate::api::GoApi;
use crate::args::{CACHE_ARG, CHECKSUM_ARG, DATA_DIR_ARG, FEATURES_ARG, WASM_ARG};
use crate::error::{clear_error, handle_c_error_binary, handle_c_error_default, set_error, Error};
use crate::memory::{Buffer, ByteSliceView};
use crate::querier::GoQuerier;
use crate::storage::GoStorage;

#[repr(C)]
pub struct cache_t {}

pub fn to_cache(ptr: *mut cache_t) -> Option<&'static mut Cache<GoApi, GoStorage, GoQuerier>> {
    if ptr.is_null() {
        None
    } else {
        let c = unsafe { &mut *(ptr as *mut Cache<GoApi, GoStorage, GoQuerier>) };
        Some(c)
    }
}

#[no_mangle]
pub extern "C" fn init_cache(
    data_dir: ByteSliceView,
    supported_features: ByteSliceView,
    cache_size: u32,
    instance_memory_limit: u32,
    err: Option<&mut Buffer>,
) -> *mut cache_t {
    let r = catch_unwind(|| {
        do_init_cache(
            data_dir,
            supported_features,
            cache_size,
            instance_memory_limit,
        )
    })
    .unwrap_or_else(|_| Err(Error::panic()));
    match r {
        Ok(t) => {
            clear_error();
            t as *mut cache_t
        }
        Err(e) => {
            set_error(e, err);
            std::ptr::null_mut()
        }
    }
}

fn do_init_cache(
    data_dir: ByteSliceView,
    supported_features: ByteSliceView,
    cache_size: u32,
    instance_memory_limit: u32, // in MiB
) -> Result<*mut Cache<GoApi, GoStorage, GoQuerier>, Error> {
    let dir = data_dir
        .read()
        .ok_or_else(|| Error::empty_arg(DATA_DIR_ARG))?;
    let dir_str = String::from_utf8(dir.to_vec())?;
    // parse the supported features
    let features_bin = supported_features
        .read()
        .ok_or_else(|| Error::empty_arg(FEATURES_ARG))?;
    let features_str = from_utf8(features_bin)?;
    let features = features_from_csv(features_str);
    let memory_cache_size = Size::mebi(
        cache_size
            .try_into()
            .expect("Cannot convert u32 to usize. What kind of system is this?"),
    );
    let instance_memory_limit = Size::mebi(
        instance_memory_limit
            .try_into()
            .expect("Cannot convert u32 to usize. What kind of system is this?"),
    );
    let options = CacheOptions {
        base_dir: dir_str.into(),
        supported_features: features,
        memory_cache_size,
        instance_memory_limit,
    };
    let cache = unsafe { Cache::new(options) }?;
    let out = Box::new(cache);
    Ok(Box::into_raw(out))
}

#[no_mangle]
pub extern "C" fn save_wasm(cache: *mut cache_t, wasm: Buffer, err: Option<&mut Buffer>) -> Buffer {
    let r = match to_cache(cache) {
        Some(c) => catch_unwind(AssertUnwindSafe(move || do_save_wasm(c, wasm)))
            .unwrap_or_else(|_| Err(Error::panic())),
        None => Err(Error::empty_arg(CACHE_ARG)),
    };
    let data = handle_c_error_binary(r, err);
    Buffer::from_vec(data)
}

fn do_save_wasm(
    cache: &mut Cache<GoApi, GoStorage, GoQuerier>,
    wasm: Buffer,
) -> Result<Checksum, Error> {
    let wasm = unsafe { wasm.read() }.ok_or_else(|| Error::empty_arg(WASM_ARG))?;
    let checksum = cache.save_wasm(wasm)?;
    Ok(checksum)
}

#[no_mangle]
pub extern "C" fn load_wasm(
    cache: *mut cache_t,
    checksum: Buffer,
    err: Option<&mut Buffer>,
) -> Buffer {
    let r = match to_cache(cache) {
        Some(c) => catch_unwind(AssertUnwindSafe(move || do_load_wasm(c, checksum)))
            .unwrap_or_else(|_| Err(Error::panic())),
        None => Err(Error::empty_arg(CACHE_ARG)),
    };
    let data = handle_c_error_binary(r, err);
    Buffer::from_vec(data)
}

fn do_load_wasm(
    cache: &mut Cache<GoApi, GoStorage, GoQuerier>,
    checksum: Buffer,
) -> Result<Vec<u8>, Error> {
    let checksum: Checksum = unsafe { checksum.read() }
        .ok_or_else(|| Error::empty_arg(CHECKSUM_ARG))?
        .try_into()?;
    let wasm = cache.load_wasm(&checksum)?;
    Ok(wasm)
}

#[no_mangle]
pub extern "C" fn pin(cache: *mut cache_t, checksum: Buffer, err: Option<&mut Buffer>) {
    let r = match to_cache(cache) {
        Some(c) => catch_unwind(AssertUnwindSafe(move || do_pin(c, checksum)))
            .unwrap_or_else(|_| Err(Error::panic())),
        None => Err(Error::empty_arg(CACHE_ARG)),
    };
    handle_c_error_default(r, err);
}

fn do_pin(cache: &mut Cache<GoApi, GoStorage, GoQuerier>, checksum: Buffer) -> Result<(), Error> {
    let checksum: Checksum = unsafe { checksum.read() }
        .ok_or_else(|| Error::empty_arg(CHECKSUM_ARG))?
        .try_into()?;
    cache.pin(&checksum)?;
    Ok(())
}

#[no_mangle]
pub extern "C" fn unpin(cache: *mut cache_t, checksum: Buffer, err: Option<&mut Buffer>) {
    let r = match to_cache(cache) {
        Some(c) => catch_unwind(AssertUnwindSafe(move || do_unpin(c, checksum)))
            .unwrap_or_else(|_| Err(Error::panic())),
        None => Err(Error::empty_arg(CACHE_ARG)),
    };
    handle_c_error_default(r, err);
}

fn do_unpin(cache: &mut Cache<GoApi, GoStorage, GoQuerier>, checksum: Buffer) -> Result<(), Error> {
    let checksum: Checksum = unsafe { checksum.read() }
        .ok_or_else(|| Error::empty_arg(CHECKSUM_ARG))?
        .try_into()?;
    cache.unpin(&checksum)?;
    Ok(())
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq)]
pub struct AnalysisReport {
    pub has_ibc_entry_points: bool,
}

impl From<cosmwasm_vm::AnalysisReport> for AnalysisReport {
    fn from(report: cosmwasm_vm::AnalysisReport) -> Self {
        AnalysisReport {
            has_ibc_entry_points: report.has_ibc_entry_points,
        }
    }
}

#[no_mangle]
pub extern "C" fn analyze_code(
    cache: *mut cache_t,
    checksum: Buffer,
    err: Option<&mut Buffer>,
) -> AnalysisReport {
    let r = match to_cache(cache) {
        Some(c) => catch_unwind(AssertUnwindSafe(move || do_analyze_code(c, checksum)))
            .unwrap_or_else(|_| Err(Error::panic())),
        None => Err(Error::empty_arg(CACHE_ARG)),
    };
    match r {
        Ok(value) => {
            clear_error();
            value
        }
        Err(error) => {
            set_error(error, err);
            AnalysisReport::default()
        }
    }
}

fn do_analyze_code(
    cache: &mut Cache<GoApi, GoStorage, GoQuerier>,
    checksum: Buffer,
) -> Result<AnalysisReport, Error> {
    let checksum: Checksum = unsafe { checksum.read() }
        .ok_or_else(|| Error::empty_arg(CHECKSUM_ARG))?
        .try_into()?;
    let report = cache.analyze(&checksum)?;
    Ok(report.into())
}

/// frees a cache reference
///
/// # Safety
///
/// This must be called exactly once for any `*cache_t` returned by `init_cache`
/// and cannot be called on any other pointer.
#[no_mangle]
pub extern "C" fn release_cache(cache: *mut cache_t) {
    if !cache.is_null() {
        // this will free cache when it goes out of scope
        let _ = unsafe { Box::from_raw(cache as *mut Cache<GoApi, GoStorage, GoQuerier>) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    static HACKATOM: &[u8] = include_bytes!("../api/testdata/hackatom.wasm");
    static IBC_REFLECT: &[u8] = include_bytes!("../api/testdata/ibc_reflect.wasm");

    #[test]
    fn init_cache_and_release_cache_work() {
        let dir: String = TempDir::new().unwrap().path().to_str().unwrap().to_owned();
        let mut err = Buffer::default();
        let features: &[u8] = b"staking";
        let cache_ptr = init_cache(
            ByteSliceView::new(Some(dir.as_bytes())),
            ByteSliceView::new(Some(features)),
            512,
            32,
            Some(&mut err),
        );
        assert_eq!(err.len, 0);
        release_cache(cache_ptr);
    }

    #[test]
    fn init_cache_writes_error() {
        let dir: String = String::from("borken\0dir"); // null bytes are valid UTF8 but not allowed in FS paths
        let mut err = Buffer::default();
        let features: &[u8] = b"staking";
        let cache_ptr = init_cache(
            ByteSliceView::new(Some(dir.as_bytes())),
            ByteSliceView::new(Some(features)),
            512,
            32,
            Some(&mut err),
        );
        assert!(cache_ptr.is_null());
        assert_ne!(err.len, 0);
        let msg = String::from_utf8(unsafe { err.consume() }).unwrap();
        assert_eq!(msg, "Error calling the VM: Cache error: Error creating Wasm dir for cache: data provided contains a nul byte");
    }

    #[test]
    fn save_wasm_works() {
        let dir: String = TempDir::new().unwrap().path().to_str().unwrap().to_owned();
        let mut err = Buffer::default();
        let features: &[u8] = b"staking";
        let cache_ptr = init_cache(
            ByteSliceView::new(Some(dir.as_bytes())),
            ByteSliceView::new(Some(features)),
            512,
            32,
            Some(&mut err),
        );
        assert_eq!(err.len, 0);

        save_wasm(cache_ptr, HACKATOM.into(), Some(&mut err));
        assert_eq!(err.len, 0);

        release_cache(cache_ptr);
    }

    #[test]
    fn load_wasm_works() {
        let dir: String = TempDir::new().unwrap().path().to_str().unwrap().to_owned();
        let mut err = Buffer::default();
        let features: &[u8] = b"staking";
        let cache_ptr = init_cache(
            ByteSliceView::new(Some(dir.as_bytes())),
            ByteSliceView::new(Some(features)),
            512,
            32,
            Some(&mut err),
        );
        assert_eq!(err.len, 0);

        let checksum = save_wasm(cache_ptr, HACKATOM.into(), Some(&mut err));
        assert_eq!(err.len, 0);

        let wasm = load_wasm(cache_ptr, checksum, Some(&mut err));
        assert_eq!(unsafe { wasm.consume() }, HACKATOM);

        release_cache(cache_ptr);
    }

    #[test]
    fn pin_works() {
        let dir: String = TempDir::new().unwrap().path().to_str().unwrap().to_owned();
        let mut err = Buffer::default();
        let features: &[u8] = b"staking";
        let cache_ptr = init_cache(
            ByteSliceView::new(Some(dir.as_bytes())),
            ByteSliceView::new(Some(features)),
            512,
            32,
            Some(&mut err),
        );
        assert_eq!(err.len, 0);

        let checksum = save_wasm(cache_ptr, HACKATOM.into(), Some(&mut err));
        assert_eq!(err.len, 0);

        pin(cache_ptr, checksum, Some(&mut err));
        assert_eq!(err.len, 0);

        // pinning again has no effect
        pin(cache_ptr, checksum, Some(&mut err));
        assert_eq!(err.len, 0);

        release_cache(cache_ptr);
    }

    #[test]
    fn unpin_works() {
        let dir: String = TempDir::new().unwrap().path().to_str().unwrap().to_owned();
        let mut err = Buffer::default();
        let features: &[u8] = b"staking";
        let cache_ptr = init_cache(
            ByteSliceView::new(Some(dir.as_bytes())),
            ByteSliceView::new(Some(features)),
            512,
            32,
            Some(&mut err),
        );
        assert_eq!(err.len, 0);

        let checksum = save_wasm(cache_ptr, HACKATOM.into(), Some(&mut err));
        assert_eq!(err.len, 0);

        pin(cache_ptr, checksum, Some(&mut err));
        assert_eq!(err.len, 0);

        unpin(cache_ptr, checksum, Some(&mut err));
        assert_eq!(err.len, 0);

        // Unpinning again has no effect
        unpin(cache_ptr, checksum, Some(&mut err));
        assert_eq!(err.len, 0);

        release_cache(cache_ptr);
    }

    #[test]
    fn analyze_code_works() {
        let dir: String = TempDir::new().unwrap().path().to_str().unwrap().to_owned();
        let mut err = Buffer::default();
        let features: &[u8] = b"stargate";
        let cache_ptr = init_cache(
            ByteSliceView::new(Some(dir.as_bytes())),
            ByteSliceView::new(Some(features)),
            512,
            32,
            Some(&mut err),
        );
        assert_eq!(err.len, 0);

        let checksum_hackatom = save_wasm(cache_ptr, HACKATOM.into(), Some(&mut err));
        assert_eq!(err.len, 0);
        let checksum_ibc_reflect = save_wasm(cache_ptr, IBC_REFLECT.into(), Some(&mut err));
        assert_eq!(err.len, 0);

        let hackatom_report = analyze_code(cache_ptr, checksum_hackatom, Some(&mut err));
        assert_eq!(
            hackatom_report,
            AnalysisReport {
                has_ibc_entry_points: false
            }
        );
        let ibc_reflect_report = analyze_code(cache_ptr, checksum_ibc_reflect, Some(&mut err));
        assert_eq!(
            ibc_reflect_report,
            AnalysisReport {
                has_ibc_entry_points: true
            }
        );

        release_cache(cache_ptr);
    }
}
