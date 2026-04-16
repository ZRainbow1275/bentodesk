//! COM `IDataObject` implementation for OLE drag-and-drop.
//!
//! Provides file paths in CF_HDROP format so that Windows Explorer and other
//! drop targets can receive files dragged out of BentoDesk zones.

use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::{
            Com::*,
            Memory::*,
            Ole::*,
        },
    },
};

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

    /// Build a CF_HDROP `HGLOBAL` containing all file paths as a DROPFILES structure.
    ///
    /// # Safety
    /// Allocates global memory and writes DROPFILES + wide-char file paths into it.
    /// The caller is responsible for freeing via `ReleaseStgMedium` or `GlobalFree`.
    unsafe fn build_hdrop(&self) -> Result<HGLOBAL> {
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

        let hglobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total_size)?;
        let raw_ptr = GlobalLock(hglobal);
        if raw_ptr.is_null() {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        let ptr = raw_ptr as *mut u8;

        // Write DROPFILES header manually (20 bytes)
        // pFiles: offset to file list
        let p_files: u32 = header_size as u32;
        std::ptr::copy_nonoverlapping(
            &p_files as *const u32 as *const u8,
            ptr,
            4,
        );
        // pt.x = 0, pt.y = 0, fNC = 0 (already zeroed by GMEM_ZEROINIT)
        // fWide = 1 (Unicode)
        let f_wide: u32 = 1;
        std::ptr::copy_nonoverlapping(
            &f_wide as *const u32 as *const u8,
            ptr.add(16),
            4,
        );

        // Write file paths sequentially after the header
        let mut offset = header_size;
        for path in &self.file_paths {
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let byte_len = wide.len() * 2;
            let src = std::slice::from_raw_parts(wide.as_ptr() as *const u8, byte_len);
            std::ptr::copy_nonoverlapping(src.as_ptr(), ptr.add(offset), byte_len);
            offset += byte_len;
        }

        let _ = GlobalUnlock(hglobal);
        Ok(hglobal)
    }
}

impl IDataObject_Impl for BentoDataObject_Impl {
    /// Retrieve data in the requested format.
    ///
    /// Only CF_HDROP via TYMED_HGLOBAL is supported; other formats return
    /// `DV_E_FORMATETC`.
    fn GetData(&self, pformatetcin: *const FORMATETC) -> Result<STGMEDIUM> {
        unsafe {
            let fmt = &*pformatetcin;
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
            } else {
                Err(Error::from(DV_E_FORMATETC))
            }
        }
    }

    /// Check whether the requested format is supported.
    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        unsafe {
            let fmt = &*pformatetc;
            if fmt.cfFormat == CF_HDROP.0 {
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

    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> Result<()> {
        Err(Error::from(E_NOTIMPL))
    }

    fn EnumFormatEtc(&self, _: u32) -> Result<IEnumFORMATETC> {
        Err(Error::from(E_NOTIMPL))
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
