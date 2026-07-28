//! `IDataObject` implementation for OLE drag-and-drop.
//!
//! Wraps a list of absolute file paths and exposes them via `CF_HDROP` inside
//! an `HGLOBAL`-backed `STGMEDIUM`. Lifted verbatim from 1.x with the unsafe
//! HGLOBAL build-out preserved (the layout is fixed by Win32 ABI).

// `IDataObject_Impl` trait methods (`GetData` / `QueryGetData` / ...) take
// raw pointers as part of the COM ABI; the trait itself does not mark them
// `unsafe`, so we cannot either. Each method body wraps the pointer reads
// in `unsafe {}` with SAFETY comments per spec §11.1.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::{cell::Cell, path::Path};

use windows::{
    Win32::{
        Foundation::*,
        System::{
            Com::StructuredStorage::CreateStreamOnHGlobal, Com::*,
            DataExchange::RegisterClipboardFormatW, Memory::*, Ole::*,
        },
        UI::Shell::{
            CFSTR_DROPDESCRIPTION, CFSTR_INDRAGLOOP, CFSTR_PREFERREDDROPEFFECT, CFSTR_SHELLIDLIST,
            CFSTR_SHELLIDLISTOFFSET, CIDLData_CreateFromIDArray, Common::ITEMIDLIST,
            DROPDESCRIPTION, ILCreateFromPathW, ILFindLastID, ILFree, ILGetSize,
        },
    },
    core::{w, *},
};

const EMPTY_DESKTOP_PIDL: [u8; 2] = [0, 0];

fn drag_proof_log_enabled() -> bool {
    std::env::var_os("BENTODESK_DRAG_PROOF_LOG").is_some()
}

fn log_drag_proof(msg: &str) {
    if drag_proof_log_enabled() {
        let mut stderr = std::io::stderr();
        let _ = std::io::Write::write_all(&mut stderr, msg.as_bytes());
        let _ = std::io::Write::flush(&mut stderr);
    }
}

mod formats;

use formats::*;

fn checked_push_bytes(target: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    let new_len = target
        .len()
        .checked_add(bytes.len())
        .ok_or_else(|| Error::from(E_OUTOFMEMORY))?;
    target.reserve(bytes.len());
    target.extend_from_slice(bytes);
    debug_assert_eq!(target.len(), new_len);
    Ok(())
}

fn push_u32_ne(target: &mut Vec<u8>, value: u32) -> Result<()> {
    checked_push_bytes(target, value.to_ne_bytes().as_slice())
}

fn build_cida_bytes(child_pidl_bytes: &[Vec<u8>]) -> Result<Vec<u8>> {
    let child_count =
        u32::try_from(child_pidl_bytes.len()).map_err(|_| Error::from(E_INVALIDARG))?;
    let offset_count = child_pidl_bytes
        .len()
        .checked_add(1)
        .ok_or_else(|| Error::from(E_OUTOFMEMORY))?;
    let header_size = core::mem::size_of::<u32>()
        .checked_add(
            offset_count
                .checked_mul(core::mem::size_of::<u32>())
                .ok_or_else(|| Error::from(E_OUTOFMEMORY))?,
        )
        .ok_or_else(|| Error::from(E_OUTOFMEMORY))?;

    let mut offsets = Vec::with_capacity(offset_count);
    let mut next_offset = header_size;
    offsets.push(u32::try_from(next_offset).map_err(|_| Error::from(E_OUTOFMEMORY))?);
    next_offset = next_offset
        .checked_add(EMPTY_DESKTOP_PIDL.len())
        .ok_or_else(|| Error::from(E_OUTOFMEMORY))?;
    for child_bytes in child_pidl_bytes {
        offsets.push(u32::try_from(next_offset).map_err(|_| Error::from(E_OUTOFMEMORY))?);
        next_offset = next_offset
            .checked_add(child_bytes.len())
            .ok_or_else(|| Error::from(E_OUTOFMEMORY))?;
    }

    let mut bytes = Vec::with_capacity(next_offset);
    push_u32_ne(&mut bytes, child_count)?;
    for offset in offsets {
        push_u32_ne(&mut bytes, offset)?;
    }
    checked_push_bytes(&mut bytes, EMPTY_DESKTOP_PIDL.as_slice())?;
    for child_bytes in child_pidl_bytes {
        checked_push_bytes(&mut bytes, child_bytes.as_slice())?;
    }
    Ok(bytes)
}

struct OwnedPidl {
    raw: *mut ITEMIDLIST,
}

impl OwnedPidl {
    fn from_path(path: &str) -> Result<Self> {
        let wide_path: Vec<u16> = path.encode_utf16().chain(core::iter::once(0)).collect();
        // SAFETY: `wide_path` is null-terminated and remains alive for the
        // duration of the call. `ILCreateFromPathW` returns a PIDL allocated by
        // the Shell allocator, which `OwnedPidl::drop` releases via `ILFree`.
        let raw = unsafe { ILCreateFromPathW(PCWSTR(wide_path.as_ptr())) };
        if raw.is_null() {
            Err(Error::from(E_INVALIDARG))
        } else {
            Ok(Self { raw })
        }
    }

    fn bytes(&self) -> Result<Vec<u8>> {
        // SAFETY: `self.raw` is a live PIDL returned by `ILCreateFromPathW`.
        // `ILGetSize` returns the byte size including the terminating SHITEMID.
        let size = unsafe { ILGetSize(Some(self.raw.cast_const())) };
        if size == 0 {
            return Err(Error::from(E_INVALIDARG));
        }
        let byte_count = usize::try_from(size).map_err(|_| Error::from(E_OUTOFMEMORY))?;
        // SAFETY: PIDLs are contiguous byte arrays of length `ILGetSize`.
        // Copying into an owned Vec keeps the returned HGLOBAL independent from
        // the Shell allocator lifetime.
        let bytes = unsafe { core::slice::from_raw_parts(self.raw.cast::<u8>(), byte_count) };
        Ok(bytes.to_vec())
    }
}

impl Drop for OwnedPidl {
    fn drop(&mut self) {
        // SAFETY: `self.raw` was allocated by `ILCreateFromPathW` and is freed
        // exactly once here when the RAII wrapper drops.
        unsafe { ILFree(Some(self.raw.cast_const())) };
    }
}

pub fn create_drag_data_object(file_paths: &[String]) -> Result<IDataObject> {
    if let Some(shell_object) = try_create_shell_data_object(file_paths)? {
        return Ok(shell_object);
    }
    Ok(BentoDataObject::new(file_paths.to_vec()).into())
}

fn try_create_shell_data_object(file_paths: &[String]) -> Result<Option<IDataObject>> {
    if file_paths.is_empty() {
        return Ok(None);
    }
    let Some(first_parent) = Path::new(&file_paths[0]).parent().map(Path::to_path_buf) else {
        return Ok(None);
    };
    if file_paths
        .iter()
        .any(|path| Path::new(path).parent().map(Path::to_path_buf) != Some(first_parent.clone()))
    {
        return Ok(None);
    }

    let parent_string = first_parent.to_string_lossy().into_owned();
    let parent_pidl = OwnedPidl::from_path(parent_string.as_str())?;
    let mut file_pidls = Vec::with_capacity(file_paths.len());
    let mut child_pidl_ptrs = Vec::with_capacity(file_paths.len());

    for path in file_paths {
        let file_pidl = OwnedPidl::from_path(path.as_str())?;
        // SAFETY: `file_pidl.raw` is a live PIDL owned by the RAII wrapper and
        // remains valid until the shell call below returns.
        let child_pidl = unsafe { ILFindLastID(file_pidl.raw.cast_const()) };
        if child_pidl.is_null() {
            return Ok(None);
        }
        file_pidls.push(file_pidl);
        child_pidl_ptrs.push(child_pidl.cast_const());
    }

    // SAFETY: all PIDLs remain alive for the duration of the shell call, and
    // the API clones the ID array data into its own object before returning.
    let data_object = unsafe {
        CIDLData_CreateFromIDArray(
            parent_pidl.raw.cast_const(),
            Some(child_pidl_ptrs.as_slice()),
        )?
    };
    drop(file_pidls);
    drop(parent_pidl);
    Ok(Some(data_object))
}

#[implement(IEnumFORMATETC)]
struct BentoFormatEnumerator {
    preferred_drop_effect: u16,
    index: Cell<usize>,
}

impl BentoFormatEnumerator {
    fn new(preferred_drop_effect: u16, index: usize) -> Self {
        Self {
            preferred_drop_effect,
            index: Cell::new(index),
        }
    }
}

impl IEnumFORMATETC_Impl for BentoFormatEnumerator_Impl {
    fn Next(&self, celt: u32, rgelt: *mut FORMATETC, pceltfetched: *mut u32) -> HRESULT {
        log_drag_proof(
            format!(
                "drag_drop: IEnumFORMATETC::Next celt={} index={}\n",
                celt,
                self.index.get()
            )
            .as_str(),
        );
        if celt > 0 && rgelt.is_null() {
            return E_POINTER;
        }
        let mut fetched = 0;
        while fetched < celt {
            let current = self.index.get();
            let Some(format) = supported_format_at(current) else {
                break;
            };
            // SAFETY: `rgelt` is non-null when `celt > 0`, checked above.
            unsafe {
                rgelt.add(fetched as usize).write(format);
            }
            self.index.set(current + 1);
            fetched += 1;
        }
        if !pceltfetched.is_null() {
            // SAFETY: COM caller supplied an optional out pointer.
            unsafe { pceltfetched.write(fetched) };
        }
        log_drag_proof(
            format!(
                "drag_drop: IEnumFORMATETC::Next fetched={} next_index={}\n",
                fetched,
                self.index.get()
            )
            .as_str(),
        );
        if fetched == celt { S_OK } else { S_FALSE }
    }

    fn Skip(&self, celt: u32) -> Result<()> {
        log_drag_proof(
            format!(
                "drag_drop: IEnumFORMATETC::Skip celt={} index={}\n",
                celt,
                self.index.get()
            )
            .as_str(),
        );
        let next = self.index.get().saturating_add(celt as usize).min(5);
        self.index.set(next);
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        log_drag_proof("drag_drop: IEnumFORMATETC::Reset\n");
        self.index.set(0);
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumFORMATETC> {
        log_drag_proof(
            format!(
                "drag_drop: IEnumFORMATETC::Clone index={}\n",
                self.index.get()
            )
            .as_str(),
        );
        Ok(BentoFormatEnumerator::new(self.preferred_drop_effect, self.index.get()).into())
    }
}

/// COM object implementing `IDataObject` for file drag operations.
///
/// Wraps a list of absolute file paths and exposes them via the CF_HDROP
/// clipboard format inside an `HGLOBAL`-backed `STGMEDIUM`.
#[implement(IDataObject)]
pub struct BentoDataObject {
    file_paths: Vec<String>,
}

impl BentoDataObject {
    /// Create a new data object containing the given file paths.
    pub fn new(paths: Vec<String>) -> Self {
        Self { file_paths: paths }
    }

    unsafe fn build_hglobal_from_bytes(bytes: &[u8]) -> Result<HGLOBAL> {
        if bytes.is_empty() {
            return Err(Error::from(E_INVALIDARG));
        }
        let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()) }?;
        let raw_ptr = unsafe { GlobalLock(hglobal) };
        if raw_ptr.is_null() {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        // SAFETY: the allocation size equals `bytes.len()`, `raw_ptr` remains
        // valid until `GlobalUnlock`, and source/destination do not overlap.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), raw_ptr as *mut u8, bytes.len());
        }
        let _ = unsafe { GlobalUnlock(hglobal) };
        Ok(hglobal)
    }

    unsafe fn build_hglobal_from_u32(value: u32) -> Result<HGLOBAL> {
        let bytes = value.to_ne_bytes();
        unsafe { Self::build_hglobal_from_bytes(bytes.as_slice()) }
    }

    unsafe fn build_zeroed_hglobal<T>() -> Result<HGLOBAL> {
        let size = core::mem::size_of::<T>();
        let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size) }?;
        let raw_ptr = unsafe { GlobalLock(hglobal) };
        if raw_ptr.is_null() {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        let _ = unsafe { GlobalUnlock(hglobal) };
        Ok(hglobal)
    }

    unsafe fn build_empty_stream(&self) -> Result<IStream> {
        let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, 1) }?;
        // SAFETY: the HGLOBAL was allocated above and is transferred to the
        // returned stream, which owns it when `fDeleteOnRelease` is true.
        unsafe { CreateStreamOnHGlobal(hglobal, true) }
    }

    /// Build a CF_HDROP `HGLOBAL` containing all file paths as a DROPFILES
    /// structure.
    ///
    /// # Safety
    ///
    /// Allocates global memory and writes a 20-byte DROPFILES header followed
    /// by the wide-char paths into it. The caller is responsible for freeing
    /// via `ReleaseStgMedium` (OLE invokes that via the STGMEDIUM lifetime).
    unsafe fn build_hdrop(&self) -> Result<HGLOBAL> {
        // SAFETY: GlobalAlloc / GlobalLock / GlobalUnlock are all FFI calls
        // into kernel32 heap APIs; the only invariants are (a) we free what
        // we lock and (b) we never write past the size we allocated. We
        // compute `total_size` exactly from the data we'll write below.

        // DROPFILES header size is 20 bytes:
        //   pFiles: u32, pt: POINT (2 x u32), fNC: BOOL (u32), fWide: BOOL (u32)
        let header_size: usize = 20;

        // Calculate total buffer size
        let mut total_size = header_size;
        for path in &self.file_paths {
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            total_size += wide.len() * 2;
        }
        total_size += 2; // Double null terminator

        // SAFETY: GMEM_MOVEABLE | GMEM_ZEROINIT is the documented combination
        // for HDROP buffers consumed by Explorer. Failure is propagated as
        // an HRESULT via `?`.
        let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total_size) }?;
        // SAFETY: hglobal is valid by the line above; GlobalLock returns NULL
        // only on failure, which we test below.
        let raw_ptr = unsafe { GlobalLock(hglobal) };
        if raw_ptr.is_null() {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        let ptr = raw_ptr as *mut u8;

        // SAFETY: ptr points at `total_size` bytes of zeroed-out memory; the
        // 20-byte DROPFILES header fits and is followed by `total_size - 20`
        // bytes of room for the path payload computed above.

        // Write DROPFILES header manually (20 bytes)
        // pFiles: offset to file list
        let p_files: u32 = header_size as u32;
        // SAFETY: `ptr` has at least 4 bytes of space (header_size = 20).
        unsafe { std::ptr::copy_nonoverlapping(&p_files as *const u32 as *const u8, ptr, 4) };
        // pt.x = 0, pt.y = 0, fNC = 0 (already zeroed by GMEM_ZEROINIT)
        // fWide = 1 (Unicode)
        let f_wide: u32 = 1;
        // SAFETY: header_size > 16+4, so writing 4 bytes at +16 stays in bounds.
        unsafe {
            std::ptr::copy_nonoverlapping(&f_wide as *const u32 as *const u8, ptr.add(16), 4)
        };

        // Write file paths sequentially after the header
        let mut offset = header_size;
        for path in &self.file_paths {
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let byte_len = wide.len() * 2;
            // SAFETY: total_size was computed as header_size + Σ(byte_len) + 2,
            // so each per-path write at `offset..offset+byte_len` stays within
            // the allocation.
            let src = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, byte_len) };
            unsafe {
                std::ptr::copy_nonoverlapping(src.as_ptr(), ptr.add(offset), byte_len);
            }
            offset += byte_len;
        }

        // SAFETY: hglobal is the value GlobalLock returned a handle for.
        let _ = unsafe { GlobalUnlock(hglobal) };
        Ok(hglobal)
    }

    unsafe fn build_drop_effect(&self, effect: DROPEFFECT) -> Result<HGLOBAL> {
        let hglobal =
            unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, core::mem::size_of::<u32>()) }?;
        let raw_ptr = unsafe { GlobalLock(hglobal) };
        if raw_ptr.is_null() {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        let value = effect.0;
        // SAFETY: the allocation is exactly one DWORD and `raw_ptr` is valid
        // until `GlobalUnlock` below.
        unsafe {
            std::ptr::copy_nonoverlapping(
                &value as *const u32 as *const u8,
                raw_ptr as *mut u8,
                core::mem::size_of::<u32>(),
            );
        }
        let _ = unsafe { GlobalUnlock(hglobal) };
        Ok(hglobal)
    }

    unsafe fn build_shell_id_list_array(&self) -> Result<HGLOBAL> {
        if self.file_paths.is_empty() {
            return Err(Error::from(E_INVALIDARG));
        }
        let mut child_pidls = Vec::with_capacity(self.file_paths.len());
        for path in &self.file_paths {
            let pidl = OwnedPidl::from_path(path)?;
            child_pidls.push(pidl);
        }
        let mut child_pidl_bytes = Vec::with_capacity(child_pidls.len());
        for pidl in &child_pidls {
            child_pidl_bytes.push(pidl.bytes()?);
        }
        let bytes = build_cida_bytes(child_pidl_bytes.as_slice())?;
        unsafe { Self::build_hglobal_from_bytes(bytes.as_slice()) }
    }

    unsafe fn build_shell_object_offsets(&self) -> Result<HGLOBAL> {
        let point_count = self.file_paths.len().saturating_add(1).max(1);
        let byte_count = point_count * core::mem::size_of::<POINT>();
        let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, byte_count) }?;
        let raw_ptr = unsafe { GlobalLock(hglobal) };
        if raw_ptr.is_null() {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        let points = vec![POINT { x: 0, y: 0 }; point_count];
        // SAFETY: the allocation is exactly `point_count * size_of::<POINT>()`
        // bytes and the source slice contains the same number of POINT values.
        unsafe {
            std::ptr::copy_nonoverlapping(
                points.as_ptr() as *const u8,
                raw_ptr as *mut u8,
                byte_count,
            );
        }
        let _ = unsafe { GlobalUnlock(hglobal) };
        Ok(hglobal)
    }

    unsafe fn build_drop_description(&self) -> Result<HGLOBAL> {
        unsafe { Self::build_zeroed_hglobal::<DROPDESCRIPTION>() }
    }

    unsafe fn build_drag_image_bits(&self) -> Result<HGLOBAL> {
        unsafe { Self::build_zeroed_hglobal::<u32>() }
    }
}

impl IDataObject_Impl for BentoDataObject_Impl {
    /// Retrieve data in the requested format.
    ///
    /// Only CF_HDROP via TYMED_HGLOBAL is supported; other formats return
    /// `DV_E_FORMATETC`.
    fn GetData(&self, pformatetcin: *const FORMATETC) -> Result<STGMEDIUM> {
        // SAFETY: `pformatetcin` is supplied by OLE. Per `IDataObject::GetData`
        // contract it is a valid `*const FORMATETC` for the lifetime of the
        // call. Dereference + field reads are sound.
        unsafe {
            let fmt = &*pformatetcin;
            let preferred_drop_effect = preferred_drop_effect_format();
            let shell_id_list_array = shell_id_list_array_format();
            let shell_object_offsets = shell_object_offsets_format();
            let drag_context = drag_context_format();
            let drag_source_helper_flags = drag_source_helper_flags_format();
            let drag_image_bits = drag_image_bits_format();
            let is_showing_text = is_showing_text_format();
            let using_default_drag_image = using_default_drag_image_format();
            let is_computing_image = is_computing_image_format();
            let disable_drag_text = disable_drag_text_format();
            let is_showing_layered = is_showing_layered_format();
            let drop_description = drop_description_format();
            let in_shell_drag_loop = in_shell_drag_loop_format();
            log_drag_proof(
                format!(
                    "drag_drop: IDataObject::GetData cf={} aspect={} tymed={}\n",
                    fmt.cfFormat, fmt.dwAspect, fmt.tymed
                )
                .as_str(),
            );
            if fmt.cfFormat == CF_HDROP.0
                && fmt.dwAspect == DVASPECT_CONTENT.0
                && fmt.tymed & TYMED_HGLOBAL.0 as u32 != 0
            {
                let hglobal = self.build_hdrop()?;
                Ok(STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: STGMEDIUM_0 { hGlobal: hglobal },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                })
            } else if fmt.cfFormat == shell_object_offsets
                && fmt.dwAspect == DVASPECT_CONTENT.0
                && fmt.tymed & TYMED_HGLOBAL.0 as u32 != 0
            {
                let hglobal = self.build_shell_object_offsets()?;
                Ok(STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: STGMEDIUM_0 { hGlobal: hglobal },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                })
            } else if fmt.cfFormat == shell_id_list_array
                && fmt.dwAspect == DVASPECT_CONTENT.0
                && fmt.tymed & TYMED_HGLOBAL.0 as u32 != 0
            {
                let hglobal = self.build_shell_id_list_array()?;
                Ok(STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: STGMEDIUM_0 { hGlobal: hglobal },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                })
            } else if fmt.cfFormat == preferred_drop_effect
                && fmt.dwAspect == DVASPECT_CONTENT.0
                && fmt.tymed & TYMED_HGLOBAL.0 as u32 != 0
            {
                let hglobal = self.build_drop_effect(DROPEFFECT_COPY)?;
                Ok(STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: STGMEDIUM_0 { hGlobal: hglobal },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                })
            } else if fmt.cfFormat == drag_context
                && fmt.dwAspect == DVASPECT_CONTENT.0
                && fmt.tymed & TYMED_ISTREAM.0 as u32 != 0
            {
                let stream = self.build_empty_stream()?;
                Ok(STGMEDIUM {
                    tymed: TYMED_ISTREAM.0 as u32,
                    u: STGMEDIUM_0 {
                        pstm: std::mem::ManuallyDrop::new(Some(stream)),
                    },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                })
            } else if fmt.cfFormat == drag_source_helper_flags
                || fmt.cfFormat == drag_image_bits
                || fmt.cfFormat == is_showing_text
                || fmt.cfFormat == using_default_drag_image
                || fmt.cfFormat == is_computing_image
                || fmt.cfFormat == disable_drag_text
                || fmt.cfFormat == is_showing_layered
                || fmt.cfFormat == drop_description
                || fmt.cfFormat == in_shell_drag_loop
            {
                let hglobal = if fmt.cfFormat == drop_description {
                    self.build_drop_description()?
                } else if fmt.cfFormat == drag_image_bits {
                    self.build_drag_image_bits()?
                } else if fmt.cfFormat == using_default_drag_image
                    || fmt.cfFormat == in_shell_drag_loop
                {
                    BentoDataObject::build_hglobal_from_u32(1)?
                } else {
                    BentoDataObject::build_hglobal_from_u32(0)?
                };
                Ok(STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: STGMEDIUM_0 { hGlobal: hglobal },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                })
            } else {
                Err(Error::from(DV_E_FORMATETC))
            }
        }
    }

    /// Check whether the requested format is supported.
    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        // SAFETY: see `GetData` — OLE-provided pointer, valid for the call.
        unsafe {
            let fmt = &*pformatetc;
            log_drag_proof(
                format!(
                    "drag_drop: IDataObject::QueryGetData cf={} aspect={} tymed={}\n",
                    fmt.cfFormat, fmt.dwAspect, fmt.tymed
                )
                .as_str(),
            );
            if format_is_supported(fmt) {
                S_OK
            } else {
                DV_E_FORMATETC
            }
        }
    }

    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> Result<()> {
        Err(Error::from(E_NOTIMPL))
    }

    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, _: *mut FORMATETC) -> HRESULT {
        DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self,
        pformatetc: *const FORMATETC,
        pmedium: *const STGMEDIUM,
        frelease: BOOL,
    ) -> Result<()> {
        unsafe {
            let fmt = &*pformatetc;
            log_drag_proof(
                format!(
                    "drag_drop: IDataObject::SetData cf={} aspect={} tymed={} release={}\n",
                    fmt.cfFormat,
                    fmt.dwAspect,
                    fmt.tymed,
                    frelease.as_bool()
                )
                .as_str(),
            );
            if !format_is_supported(fmt) {
                return Err(Error::from(DV_E_FORMATETC));
            }
            if frelease.as_bool() && !pmedium.is_null() {
                let medium = &*pmedium;
                if medium.tymed != TYMED_NULL.0 as u32 {
                    // SAFETY: `ReleaseStgMedium` only needs a valid pointer to
                    // the caller-provided `STGMEDIUM`.
                    ReleaseStgMedium(pmedium.cast_mut());
                }
            }
            Ok(())
        }
    }

    fn EnumFormatEtc(&self, dw_direction: u32) -> Result<IEnumFORMATETC> {
        log_drag_proof(
            format!("drag_drop: IDataObject::EnumFormatEtc dir={dw_direction}\n").as_str(),
        );
        if dw_direction != DATADIR_GET.0 as u32 {
            return Err(Error::from(E_NOTIMPL));
        }
        log_drag_proof("drag_drop: IDataObject::EnumFormatEtc returning enum\n");
        Ok(BentoFormatEnumerator::new(preferred_drop_effect_format(), 0).into())
    }

    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Option<&IAdviseSink>) -> Result<u32> {
        Err(Error::from(OLE_E_ADVISENOTSUPPORTED))
    }

    fn DUnadvise(&self, _: u32) -> Result<()> {
        Err(Error::from(OLE_E_ADVISENOTSUPPORTED))
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(Error::from(OLE_E_ADVISENOTSUPPORTED))
    }
}

#[cfg(test)]
mod tests;
