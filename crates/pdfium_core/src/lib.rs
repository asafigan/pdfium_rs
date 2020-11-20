//! `pdfium_core` is a safe and minimal Rust wrapper around the PDFium library.
//!
//! ## Example
//! Here is an example of getting the number of pages in a PDF:
//! ```no_run
//! use pdfium_core::Library;
//! use std::path::Path;
//!
//! let mut library = Library::init_library().unwrap();
//!
//! // empty password
//! let password = [];
//! let document_handle = library
//!     .load_document(Path::new("example.pdf"), password)
//!     .unwrap()
//!     .unwrap();
//!
//! println!("{}", library.get_page_count(&document_handle));
//! ```
//!
//! The first thing to notice is that all methods are implemented on the [`Library`] struct.
//! This is because of two reasons: the PDFium library must be initialize before using it and
//! it is not thread safe. Modeling the PDFium library as a resource ensures that is must be initialized
//! before being used. Also all methods require a mutable reference to the library to ensure that
//! synchronization has occurred before calling any method in the library.
//!
//! ## Initializing the library
//! Another thing to notice is that [`Library::init_library()`] returns an option. This is because PDFium can only
//! be initialized once per process without being uninitialized first. The library will be
//! uninitialized when the Library struct is dropped.
//!
//! For example:
//! ```
//! use pdfium_core::Library;
//!
//! let library = Library::init_library();
//! assert!(library.is_some());
//!
//! assert!(Library::init_library().is_none());
//!
//! drop(library);
//! assert!(Library::init_library().is_some());
//! ```
//!
//! ## Handles
//! `pdfium_core` uses handles that wrap non-null pointers in order to manage the resources
//! used by PDFium. All of the handles will track the correct lifetimes of the underlying resources
//! and will clean up these resources when they are dropped.
//!
//! For example:
//! ```no_run
//! let mut library = Library::init_library();
//!
//! use pdfium_core::Library;
//! use std::path::Path;
//!
//! let mut library = Library::init_library().unwrap();
//!
//! let document_handle = library
//!     .load_document(Path::new("example.pdf"), [])
//!     .unwrap()
//!     .unwrap();
//!
//! // load first page
//! let page_handle = library.load_page(&document_handle, 0).unwrap();
//!
//! // can't drop the document_handle before the page_handle because
//! // the page can't outlive its parent document.
//!
//! // uncommenting the next line would cause a compile time error.
//! // drop(document_handle);
//! drop(page_handle);
//! ```

use std::ffi::{c_void, CString, NulError};
use std::marker::PhantomData;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

/// A properly initialized instance of the PDFium library.
/// 
/// The PDFium library is not thread safe so there can only be one instance per process.
pub struct Library(PhantomData<()>);

static INITIALIZED: AtomicBool = AtomicBool::new(false);

impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDF_DestroyLibrary();
        }
        INITIALIZED.store(false, Ordering::Relaxed);
    }
}

impl Library {
    pub fn init_library() -> Option<Library> {
        let already_initialized = INITIALIZED.compare_and_swap(false, true, Ordering::SeqCst);

        if already_initialized {
            None
        } else {
            unsafe {
                pdfium_bindings::FPDF_InitLibrary();
            }
            Some(Library(Default::default()))
        }
    }

    /// Get last last error code when a function fails.
    /// 
    /// If the previous PDFium call succeeded, the value will be `None`.
    pub fn get_last_error(&mut self) -> Option<PdfiumError> {
        PdfiumError::from_code(unsafe { pdfium_bindings::FPDF_GetLastError() as u32 })
    }

    pub fn load_document(
        &mut self,
        path: &Path,
        password: impl Into<Vec<u8>>,
    ) -> Result<Option<DocumentHandle<'static>>, NulError> {
        let path = CString::new(path.to_string_lossy().to_string().into_bytes())?.as_ptr();
        let password = CString::new(password)?.as_ptr();
        let handle = NonNull::new(unsafe { pdfium_bindings::FPDF_LoadDocument(path, password) });

        Ok(handle.map(|handle| DocumentHandle {
            handle,
            life_time: Default::default(),
        }))
    }

    pub fn load_mem_document<'a>(
        &mut self,
        buffer: &'a [u8],
        password: impl Into<Vec<u8>>,
    ) -> Result<Option<DocumentHandle<'a>>, NulError> {
        let password = CString::new(password)?.as_ptr();
        let handle = NonNull::new(unsafe {
            pdfium_bindings::FPDF_LoadMemDocument(
                buffer.as_ptr() as *mut c_void,
                buffer.len() as i32,
                password,
            )
        });

        Ok(handle.map(|handle| DocumentHandle {
            handle,
            life_time: Default::default(),
        }))
    }

    pub fn get_page_count(&mut self, document: &DocumentHandle) -> usize {
        unsafe { pdfium_bindings::FPDF_GetPageCount(document.handle.as_ptr()) as usize }
    }

    pub fn create_external_bitmap<'a>(
        &mut self,
        width: usize,
        height: usize,
        format: BitmapFormat,
        buffer: &'a mut [u8],
        height_stride: usize,
    ) -> Option<BitmapHandle<'a>> {
        if buffer.len() < height * height_stride {
            return None;
        }

        let handle = NonNull::new(unsafe {
            pdfium_bindings::FPDFBitmap_CreateEx(
                width as i32,
                height as i32,
                format as i32,
                buffer.as_ptr() as *mut c_void,
                height_stride as i32,
            )
        });

        handle.map(|handle| BitmapHandle {
            handle,
            life_time: Default::default(),
        })
    }

    pub fn load_page<'a>(
        &mut self,
        document: &'a DocumentHandle,
        index: usize,
    ) -> Option<PageHandle<'a>> {
        let handle = NonNull::new(unsafe {
            pdfium_bindings::FPDF_LoadPage(document.handle.as_ptr(), index as i32)
        });

        handle.map(|handle| PageHandle {
            handle,
            life_time: Default::default(),
        })
    }

    pub fn get_page_width(&mut self, page: &PageHandle) -> f32 {
        unsafe { pdfium_bindings::FPDF_GetPageWidthF(page.handle.as_ptr()) }
    }

    pub fn get_page_height(&mut self, page: &PageHandle) -> f32 {
        unsafe { pdfium_bindings::FPDF_GetPageHeightF(page.handle.as_ptr()) }
    }

    pub fn render_page_bitmap(
        &mut self,
        bitmap: &mut BitmapHandle,
        page: &PageHandle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        orientation: PageOrientation,
        flags: i32,
    ) {
        unsafe {
            pdfium_bindings::FPDF_RenderPageBitmap(
                bitmap.handle.as_ptr(),
                page.handle.as_ptr(),
                x,
                y,
                width,
                height,
                orientation as i32,
                flags,
            );
        }
    }

    pub fn get_bitmap_width(&mut self, bitmap: &BitmapHandle) -> u32 {
        unsafe { pdfium_bindings::FPDFBitmap_GetWidth(bitmap.handle.as_ptr()) as u32 }
    }

    pub fn get_bitmap_height(&mut self, bitmap: &BitmapHandle) -> u32 {
        unsafe { pdfium_bindings::FPDFBitmap_GetHeight(bitmap.handle.as_ptr()) as u32 }
    }

    pub fn bitmap_fill_rect(
        &mut self,
        bitmap: &mut BitmapHandle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: u64,
    ) {
        unsafe {
            pdfium_bindings::FPDFBitmap_FillRect(bitmap.handle.as_ptr(), x, y, width, height, color)
        }
    }
}

pub enum BitmapFormat {
    /// Gray scale bitmap, one byte per pixel.
    GreyScale = pdfium_bindings::FPDFBitmap_Gray as isize,
    /// 3 bytes per pixel, byte order: blue, green, red.
    BGR = pdfium_bindings::FPDFBitmap_BGR as isize,
    /// 4 bytes per pixel, byte order: blue, green, red, unused.
    BGRx = pdfium_bindings::FPDFBitmap_BGRx as isize,
    /// 4 bytes per pixel, byte order: blue, green, red, alpha.
    BGRA = pdfium_bindings::FPDFBitmap_BGRA as isize,
}

pub enum PageOrientation {
    /// normal
    Normal = 0,
    /// rotated 90 degrees clockwise
    Clockwise = 1,
    /// rotated 180 degrees
    Flip = 2,
    /// rotated 90 degrees counter-clockwise
    CounterClockwise = 3,
}

/// PDFium Error Codes
pub enum PdfiumError {
    /// Unknown error.
    Unknown = pdfium_bindings::FPDF_ERR_UNKNOWN as isize,
    /// File not found or could not be opened.
    BadFile = pdfium_bindings::FPDF_ERR_FILE as isize,
    /// File not in PDF format or corrupted.
    BadFormat = pdfium_bindings::FPDF_ERR_FORMAT as isize,
    /// Password required or incorrect password.
    BadPassword = pdfium_bindings::FPDF_ERR_PASSWORD as isize,
    /// Unsupported security scheme.
    UnsupportedSecurityScheme = pdfium_bindings::FPDF_ERR_SECURITY as isize,
    /// Page not found or content error.
    BadPage = pdfium_bindings::FPDF_ERR_PAGE as isize,
}

impl PdfiumError {
    fn from_code(code: u32) -> Option<PdfiumError> {
        match code {
            pdfium_bindings::FPDF_ERR_SUCCESS => None,
            pdfium_bindings::FPDF_ERR_UNKNOWN => Some(PdfiumError::Unknown),
            pdfium_bindings::FPDF_ERR_FILE => Some(PdfiumError::BadFile),
            pdfium_bindings::FPDF_ERR_FORMAT => Some(PdfiumError::BadFormat),
            pdfium_bindings::FPDF_ERR_PASSWORD => Some(PdfiumError::BadPassword),
            pdfium_bindings::FPDF_ERR_SECURITY => Some(PdfiumError::UnsupportedSecurityScheme),
            pdfium_bindings::FPDF_ERR_PAGE => Some(PdfiumError::BadPage),
            _ => Some(PdfiumError::Unknown),
        }
    }
}

pub struct DocumentHandle<'a> {
    handle: NonNull<pdfium_bindings::fpdf_document_t__>,
    life_time: PhantomData<&'a [u8]>,
}

impl<'a> Drop for DocumentHandle<'a> {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDF_CloseDocument(self.handle.as_ptr());
        }
    }
}

pub struct PageHandle<'a> {
    handle: NonNull<pdfium_bindings::fpdf_page_t__>,
    life_time: PhantomData<&'a [u8]>,
}

impl<'a> Drop for PageHandle<'a> {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDF_ClosePage(self.handle.as_ptr());
        }
    }
}

pub struct BitmapHandle<'a> {
    handle: NonNull<pdfium_bindings::fpdf_bitmap_t__>,
    life_time: PhantomData<&'a mut [u8]>,
}

impl<'a> Drop for BitmapHandle<'a> {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDFBitmap_Destroy(self.handle.as_ptr());
        }
    }
}

#[cfg(test)]
use once_cell::sync::Lazy;
#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[cfg(test)]
mod tests {
    use super::*;

    static DUMMY_PDF: &'static [u8] = include_bytes!("../../../test_assets/dummy.pdf");

    #[test]
    fn only_one_library_at_a_time() {
        let _guard = TEST_LOCK.lock().unwrap();
        let first = Library::init_library();
        assert!(first.is_some());
        let second = Library::init_library();
        assert!(second.is_none());

        drop(first);
        let third = Library::init_library();
        assert!(third.is_some());
    }

    #[test]
    fn page_count() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut library = Library::init_library().unwrap();
        let document = library.load_mem_document(DUMMY_PDF, []).unwrap().unwrap();

        assert_eq!(library.get_page_count(&document), 1);
    }

    #[test]
    fn page_dimensions() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut library = Library::init_library().unwrap();
        let document = library.load_mem_document(DUMMY_PDF, []).unwrap().unwrap();
        let page = library.load_page(&document, 0).unwrap();

        assert_eq!(library.get_page_width(&page), 595.0);
        assert_eq!(library.get_page_height(&page), 842.0);
    }

    #[test]
    fn render() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut library = Library::init_library().unwrap();
        let document = library.load_mem_document(DUMMY_PDF, []).unwrap().unwrap();
        let page = library.load_page(&document, 0).unwrap();

        let width = library.get_page_width(&page).round() as usize;
        let height = library.get_page_height(&page).round() as usize;
        const CHANNELS: usize = 4;

        let mut buffer: Vec<u8> = vec![0xFF; CHANNELS * width * height];

        let mut bitmap = library
            .create_external_bitmap(
                width,
                height,
                BitmapFormat::BGRA,
                &mut buffer,
                width * CHANNELS,
            )
            .unwrap();

        library.render_page_bitmap(
            &mut bitmap,
            &page,
            0,
            0,
            width as i32,
            height as i32,
            PageOrientation::Normal,
            0,
        );

        assert_eq!(library.get_last_error(), 0);

        drop(bitmap);

        // There is at least one none white pixel
        assert!(buffer.iter().any(|x| *x != 0xFF));
    }
}
