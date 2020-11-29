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
//! let password = None;
//! let path = Path::new("example.pdf");
//! let document_handle = library
//!     .load_document(&path, password)
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
//! use pdfium_core::Library;
//! use std::path::Path;
//!
//! let mut library = Library::init_library();
//!
//! let mut library = Library::init_library().unwrap();
//!
//! let path = Path::new("example.pdf");
//! let document_handle = library
//!     .load_document(&path, None)
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

#![allow(clippy::too_many_arguments)]

use std::ffi::{c_void, CStr};
use std::fmt;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

/// A properly initialized instance of the PDFium library.
///
/// The PDFium library is not thread safe so there can only be one instance per process.
///
/// The PDFium library will be uninitialized when this value is dropped.
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
    /// Initialize the PDFium library.
    ///
    /// The PDFium library is not thread safe so there can only be one instance per process.
    ///
    /// Will return `None` if the library is already initialized.
    ///
    /// ## Examples
    /// Demonstration that only one instance can be initialized at a time:
    /// ```
    /// use pdfium_core::Library;
    ///
    /// let library = Library::init_library();
    /// assert!(library.is_some());
    ///
    /// assert!(Library::init_library().is_none());
    ///
    /// drop(library);
    /// assert!(Library::init_library().is_some());
    /// ```
    pub fn init_library() -> Option<Library> {
        let already_initialized = INITIALIZED.compare_and_swap(false, true, Ordering::SeqCst);

        if already_initialized {
            None
        } else {
            let config = pdfium_bindings::FPDF_LIBRARY_CONFIG_ {
                version: 2,
                m_pUserFontPaths: std::ptr::null::<*const i8>() as *mut _,
                m_pIsolate: std::ptr::null::<std::ffi::c_void>() as *mut _,
                m_v8EmbedderSlot: 0,
                m_pPlatform: std::ptr::null::<std::ffi::c_void>() as *mut _,
            };
            unsafe {
                pdfium_bindings::FPDF_InitLibraryWithConfig(&config);
            }
            Some(Library(Default::default()))
        }
    }

    /// Get last last error code when a function fails.
    ///
    /// If the previous PDFium function call succeeded, this function has undefined behavior. (From personal experience, I have found it remains unchanged.)
    fn get_last_error(&mut self) -> Option<PdfiumError> {
        PdfiumError::from_code(unsafe { pdfium_bindings::FPDF_GetLastError() as u32 })
    }

    /// Get last last error code when a function fails, if `None` default to [`Unknown`](PdfiumError::Unknown).
    ///
    /// If the previous PDFium function call succeeded, this function has undefined behavior. (From personal experience, I have found it remains unchanged.)
    fn last_error(&mut self) -> PdfiumError {
        self.get_last_error().unwrap_or(PdfiumError::Unknown)
    }

    /// Open and load a PDF document from a file path.
    ///
    /// The encoding for `password` can be either UTF-8 or Latin-1. PDFs,
    /// depending on the security handler revision, will only accept one or
    /// the other encoding. If `password`'s encoding and the PDF's expected
    /// encoding do not match, it will automatically
    /// convert `password` to the other encoding.
    ///
    /// `password` is ignored if the document is not encrypted.
    ///
    /// ## Errors
    /// This function will return an error under a number of different circumstances.
    /// Some of these error conditions are listed here, together with their [`PdfiumError`].
    /// The mapping to [`PdfiumError`]s is not part of the compatibility contract of the function,
    /// especially the [`Unknown`](PdfiumError::Unknown) kind might change to more specific kinds in the future.
    ///
    /// - [`BadFile`](PdfiumError::BadFile): Unable to find file.
    /// - [`BadFile`](PdfiumError::BadFile): Unable to open file.
    /// - [`BadFile`](PdfiumError::BadFile): Unable to convert Path to CString.
    /// - [`BadPassword`](PdfiumError::BadPassword): A password is required but there is no provided password.
    /// - [`BadPassword`](PdfiumError::BadPassword): The provided password is wrong.
    /// - [`BadFormat`](PdfiumError::BadFormat): The file contains a improperly formatted pdf.
    /// - [`BadFormat`](PdfiumError::BadFormat): The file contains no data.
    /// - [`UnsupportedSecurityScheme`](PdfiumError::UnsupportedSecurityScheme): The document is protected by an unsupported security schema.
    ///
    /// ## Examples
    /// ```no_run
    /// use pdfium_core::Library;
    /// use std::path::Path;
    /// use std::ffi::CString;
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let path = Path::new("dummy.pdf");
    /// let password = CString::new("test").unwrap();
    /// let document_handle = library.load_document(&path, Some(&password));
    /// assert!(document_handle.is_ok());
    /// ```
    pub fn load_document(
        &mut self,
        path: &Path,
        password: Option<&CStr>,
    ) -> Result<DocumentHandle<'static>, PdfiumError> {
        let password = password.map(|x| x.as_ptr()).unwrap_or_else(std::ptr::null);

        let path = cstr(path)?;

        let handle =
            NonNull::new(unsafe { pdfium_bindings::FPDF_LoadDocument(path.as_ptr(), password) });

        handle
            .map(|handle| DocumentHandle {
                handle,
                life_time: Default::default(),
            })
            .ok_or_else(|| self.last_error())
    }

    /// Open and load a PDF document from memory.
    ///
    /// See the [`load_document`](Library::load_document) function for more details.
    /// ## Examples
    /// ```
    /// use pdfium_core::Library;
    /// use std::ffi::CString;
    /// # static DUMMY_PASSWORD_PDF: &'static [u8] = include_bytes!("../../../test_assets/password.pdf");
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let password = CString::new("test").unwrap();
    /// let document_handle = library.load_mem_document(DUMMY_PASSWORD_PDF, Some(&password));
    /// assert!(document_handle.is_ok());
    /// ```
    pub fn load_mem_document<'a>(
        &mut self,
        buffer: &'a [u8],
        password: Option<&CStr>,
    ) -> Result<DocumentHandle<'a>, PdfiumError> {
        let password = password.map(|x| x.as_ptr()).unwrap_or_else(std::ptr::null);

        let handle = NonNull::new(unsafe {
            pdfium_bindings::FPDF_LoadMemDocument(
                buffer.as_ptr() as *mut c_void,
                buffer.len() as i32,
                password,
            )
        });

        handle
            .map(|handle| DocumentHandle {
                handle,
                life_time: Default::default(),
            })
            .ok_or_else(|| self.last_error())
    }

    /// Get total number of pages in the document.
    /// ## Examples
    /// ```
    /// use pdfium_core::Library;
    /// # static DUMMY_PDF: &'static [u8] = include_bytes!("../../../test_assets/dummy.pdf");
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let document_handle = library
    ///     .load_mem_document(DUMMY_PDF, None)
    ///     .unwrap();
    ///
    /// let page_count = library.get_page_count(&document_handle);
    /// assert_eq!(page_count, 1);
    /// ```
    pub fn get_page_count(&mut self, document: &DocumentHandle) -> usize {
        unsafe { pdfium_bindings::FPDF_GetPageCount(document.handle.as_ptr()) as usize }
    }

    /// Load a page inside the document.
    ///
    /// `index` 0 for the first page.
    ///
    /// ## Errors
    /// This function will return an error under a number of different circumstances.
    /// Some of these error conditions are listed here, together with their [`PdfiumError`].
    /// The mapping to [`PdfiumError`]s is not part of the compatibility contract of the function,
    /// especially the [`Unknown`](PdfiumError::Unknown) kind might change to more specific kinds in the future.
    ///
    /// - [`BadFile`](PdfiumError::BadFile): Page not found.
    /// - [`BadFile`](PdfiumError::BadFile): Content error.
    ///
    /// ## Examples
    /// ```
    /// use pdfium_core::Library;
    /// # static DUMMY_PDF: &'static [u8] = include_bytes!("../../../test_assets/dummy.pdf");
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let document_handle = library
    ///     .load_mem_document(DUMMY_PDF, None)
    ///     .unwrap();
    ///
    /// let page_handle = library.load_page(&document_handle, 0);
    /// assert!(page_handle.is_ok());
    /// ```
    pub fn load_page<'a>(
        &mut self,
        document: &'a DocumentHandle,
        index: usize,
    ) -> Result<PageHandle<'a>, PdfiumError> {
        let handle = NonNull::new(unsafe {
            pdfium_bindings::FPDF_LoadPage(document.handle.as_ptr(), index as i32)
        });

        handle
            .map(|handle| PageHandle {
                handle,
                life_time: Default::default(),
            })
            .ok_or_else(|| self.last_error())
    }

    /// Get page width.
    ///
    /// Page width (excluding non-displayable area) measured in points.
    /// One point is 1/72 inch (around 0.3528 mm).
    /// ## Examples
    /// ```
    /// use pdfium_core::Library;
    /// # static DUMMY_PDF: &'static [u8] = include_bytes!("../../../test_assets/dummy.pdf");
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let document_handle = library
    ///     .load_mem_document(DUMMY_PDF, None)
    ///     .unwrap();
    ///
    /// let page_handle = library.load_page(&document_handle, 0).unwrap();
    /// let page_width = library.get_page_width(&page_handle);
    /// assert_eq!(page_width, 595.0);
    /// ```
    pub fn get_page_width(&mut self, page: &PageHandle) -> f32 {
        unsafe { pdfium_bindings::FPDF_GetPageWidthF(page.handle.as_ptr()) }
    }

    /// Get page height.
    ///
    /// Page height (excluding non-displayable area) measured in points.
    /// One point is 1/72 inch (around 0.3528 mm).
    /// ## Examples
    /// ```
    /// use pdfium_core::Library;
    /// # static DUMMY_PDF: &'static [u8] = include_bytes!("../../../test_assets/dummy.pdf");
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let document_handle = library
    ///     .load_mem_document(DUMMY_PDF, None)
    ///     .unwrap();
    ///
    /// let page_handle = library.load_page(&document_handle, 0).unwrap();
    /// let page_height = library.get_page_height(&page_handle);
    /// assert_eq!(page_height, 842.0);
    /// ```
    pub fn get_page_height(&mut self, page: &PageHandle) -> f32 {
        unsafe { pdfium_bindings::FPDF_GetPageHeightF(page.handle.as_ptr()) }
    }

    /// Render contents of a page to a device independent bitmap.
    ///
    /// `start_x` is the x-axis coordinate in the bitmap at which to place the top-left corner of the page.
    ///
    /// `start_y` is the y-axis coordinate in the bitmap at which to place the top-left corner of the page.
    ///
    /// `width` is the width to render the page in the bitmap. `height` is the height to render the page in the bitmap. These allow scaling of the page.
    ///
    /// `orientation` is the orientation to render the page. See [`PageOrientation`] for more information.
    ///
    /// `flags` is used to control advanced rendering options. `0` or [`rendering_flags::NORMAL`] for normal display. See [`rendering_flags`] module for more information.
    ///
    /// ## Examples
    /// Render page into external buffer:
    /// ```
    /// use pdfium_core::{
    ///     BitmapFormat,
    ///     Library,
    ///     PageOrientation,
    ///     rendering_flags,
    /// };
    /// # static DUMMY_PDF: &'static [u8] = include_bytes!("../../../test_assets/dummy.pdf");
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let document_handle = library
    ///     .load_mem_document(DUMMY_PDF, None)
    ///     .unwrap();
    ///
    ///
    /// let page_handle = library.load_page(&document_handle, 0).unwrap();
    ///
    /// let width = library.get_page_width(&page_handle) as usize;
    /// let height = library.get_page_height(&page_handle) as usize;
    /// let format = BitmapFormat::BGRA;
    /// let height_stride = width * format.bytes_per_pixel();
    ///
    /// // create buffer of white pixels
    /// let mut buffer = vec![0xFF; height * height_stride];
    ///
    /// let mut bitmap_handle = library.create_external_bitmap(
    ///     width,
    ///     height,
    ///     format,
    ///     &mut buffer,
    ///     height_stride
    /// ).unwrap();
    ///
    /// library.render_page_to_bitmap(
    ///     &mut bitmap_handle,
    ///     &page_handle,
    ///     0,
    ///     0,
    ///     width as i32,
    ///     height as i32,
    ///     PageOrientation::Normal,
    ///     rendering_flags::NORMAL,
    /// );
    ///
    /// // drop the bitmap so that you can access the underlying buffer
    /// drop(bitmap_handle);
    ///
    /// // there is at least one none white pixel
    /// assert!(buffer.iter().any(|x| *x != 0xFF));
    /// ```
    pub fn render_page_to_bitmap(
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

    /// Create a device independent bitmap.
    ///
    /// `width` and `height` are the width and height of the bitmap. Both must be greater than 0.
    ///
    /// `format` is the format of the bitmap. See [`BitmapFormat`] for more information.
    ///
    /// ## Errors
    /// This function will return an error under a number of different circumstances.
    /// Some of these error conditions are listed here, together with their [`PdfiumError`].
    /// The mapping to [`PdfiumError`]s is not part of the compatibility contract of the function,
    /// especially the [`Unknown`](PdfiumError::Unknown) kind might change to more specific kinds in the future.
    ///
    /// - [`BadFormat`](PdfiumError::BadFormat): `width` or `height` is 0.
    ///
    /// ### Examples
    /// ```
    /// use pdfium_core::{Library, BitmapFormat};
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let bitmap_handle = library.create_bitmap(100, 100, BitmapFormat::BGRA);
    /// assert!(bitmap_handle.is_ok());
    /// ```
    pub fn create_bitmap<'a>(
        &mut self,
        width: usize,
        height: usize,
        format: BitmapFormat,
    ) -> Result<BitmapHandle<'a>, PdfiumError> {
        self.create_bitmap_ex(width, height, format, None, 0)
    }

    /// Create a device independent bitmap from an external buffer.
    ///
    /// Similar to [`Library::create_bitmap`], but the bitmap is stored in an external buffer.
    ///
    /// `buffer` is used to store the bytes of the buffer.
    /// The length of `buffer` must be at least `height * height_stride`.
    ///
    /// `height_stride` is the number of bytes for each scan line.
    /// A scan line is the number of bytes separating pixels in the y-direction.
    /// This input allows for buffers that have scan lines larger than `width * number_of_bytes_per_pixel`.
    ///
    /// ## Errors
    /// This function will return an error under a number of different circumstances.
    /// Some of these error conditions are listed here, together with their [`PdfiumError`].
    /// The mapping to [`PdfiumError`]s is not part of the compatibility contract of the function,
    /// especially the [`Unknown`](PdfiumError::Unknown) kind might change to more specific kinds in the future.
    ///
    /// - [`BadFormat`](PdfiumError::BadFormat): `width` or `height` is 0.
    /// - [`BadFormat`](PdfiumError::BadFormat): `buffer` is an incorrect size.
    ///
    /// ### Examples
    /// ```
    /// use pdfium_core::{Library, BitmapFormat};
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let width = 100;
    /// let height = 100;
    /// let format = BitmapFormat::BGRA;
    /// let height_stride = width * format.bytes_per_pixel();
    ///
    /// let mut buffer = vec![0xFF; height * height_stride];
    ///
    /// let bitmap_handle = library.create_external_bitmap(
    ///     width,
    ///     height,
    ///     format,
    ///     &mut buffer,
    ///     height_stride
    /// );
    /// assert!(bitmap_handle.is_ok());
    /// ```
    pub fn create_external_bitmap<'a>(
        &mut self,
        width: usize,
        height: usize,
        format: BitmapFormat,
        buffer: &'a mut [u8],
        height_stride: usize,
    ) -> Result<BitmapHandle<'a>, PdfiumError> {
        self.create_bitmap_ex(width, height, format, Some(buffer), height_stride)
    }

    /// Create a device independent bitmap.
    ///
    /// `width` and `height` are the width and height of the bitmap. Both must be greater than 0.
    ///
    /// `format` is the format of the bitmap. See [`BitmapFormat`] for more information.
    ///
    /// `buffer` is an external buffer that holds the bitmap. If this parameter is `None`, then the a new buffer will be created.
    ///
    /// For external buffer only, `height_stride` is the number of bytes for each scan line.
    /// A scan line is the number of bytes separating pixels in the y-direction.
    /// This input allows for buffers that have scan lines larger than `width * number_of_bytes_per_pixel`.
    fn create_bitmap_ex<'a>(
        &mut self,
        width: usize,
        height: usize,
        format: BitmapFormat,
        buffer: Option<&'a mut [u8]>,
        height_stride: usize,
    ) -> Result<BitmapHandle<'a>, PdfiumError> {
        let buffer = buffer
            .map(|buffer| {
                if buffer.len() < height * height_stride {
                    Err(PdfiumError::BadFormat)
                } else {
                    Ok(buffer.as_ptr())
                }
            })
            .transpose()?;

        let buffer = buffer.unwrap_or_else(std::ptr::null);

        let handle = NonNull::new(unsafe {
            pdfium_bindings::FPDFBitmap_CreateEx(
                width as i32,
                height as i32,
                format as i32,
                buffer as *mut c_void,
                height_stride as i32,
            )
        });

        handle
            .map(|handle| BitmapHandle {
                handle,
                life_time: Default::default(),
            })
            .ok_or_else(|| self.last_error())
    }

    /// Get the format of the bitmap.
    ///
    /// ## Examples
    /// ```
    /// use pdfium_core::{Library, BitmapFormat};
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let bitmap_handle = library.create_bitmap(100, 100, BitmapFormat::BGRA).unwrap();
    ///
    /// let format = library.get_bitmap_format(&bitmap_handle);
    /// assert_eq!(format, BitmapFormat::BGRA);
    /// ```
    pub fn get_bitmap_format(&mut self, bitmap: &BitmapHandle) -> BitmapFormat {
        let format = unsafe { pdfium_bindings::FPDFBitmap_GetFormat(bitmap.handle.as_ptr()) };

        BitmapFormat::from_i32(format).unwrap()
    }

    /// Get width of a bitmap in pixels.
    ///
    /// ## Examples
    /// ```
    /// use pdfium_core::{Library, BitmapFormat};
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let bitmap_handle = library.create_bitmap(100, 100, BitmapFormat::BGRA).unwrap();
    ///
    /// let width = library.get_bitmap_width(&bitmap_handle);
    /// assert_eq!(width, 100);
    /// ```
    pub fn get_bitmap_width(&mut self, bitmap: &BitmapHandle) -> u32 {
        unsafe { pdfium_bindings::FPDFBitmap_GetWidth(bitmap.handle.as_ptr()) as u32 }
    }

    /// Get height of a bitmap in pixels.
    ///
    /// ## Examples
    /// ```
    /// use pdfium_core::{Library, BitmapFormat};
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let bitmap_handle = library.create_bitmap(100, 100, BitmapFormat::BGRA).unwrap();
    ///
    /// let height = library.get_bitmap_height(&bitmap_handle);
    /// assert_eq!(height, 100);
    /// ```
    pub fn get_bitmap_height(&mut self, bitmap: &BitmapHandle) -> u32 {
        unsafe { pdfium_bindings::FPDFBitmap_GetHeight(bitmap.handle.as_ptr()) as u32 }
    }

    /// Get number of bytes for each line in the bitmap buffer.
    ///
    /// ## Examples
    /// ```
    /// use pdfium_core::{Library, BitmapFormat};
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let bitmap_handle = library.create_bitmap(100, 100, BitmapFormat::BGRA).unwrap();
    ///
    /// let stride = library.get_bitmap_stride(&bitmap_handle);
    /// assert_eq!(stride, 400);
    /// ```
    pub fn get_bitmap_stride(&mut self, bitmap: &BitmapHandle) -> usize {
        unsafe { pdfium_bindings::FPDFBitmap_GetStride(bitmap.handle.as_ptr()) as usize }
    }

    /// Fill a rectangle in a bitmap.
    ///
    /// `x` and `y` make the position of the top-left pixel of the rectangle.
    ///
    /// `width` and `height` are the dimensions of the rectangle.
    ///
    /// `color` is the fill color. It will take the lowest bytes for the color.
    /// The format of the color is dependent on the bitmap's [`BitmapFormat`].
    ///
    /// ## Example
    /// Fill the bitmap with white pixels:
    /// ```
    /// use pdfium_core::{Library, BitmapFormat};
    ///
    /// let mut library = Library::init_library().unwrap();
    ///
    /// let mut bitmap_handle = library.create_bitmap(100, 100, BitmapFormat::BGRA).unwrap();
    ///
    /// library.bitmap_fill_rect(&mut bitmap_handle, 0, 0, 100, 100, 0xFFFFFFFF);
    /// ```
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

/// PDFium Error Codes
#[repr(i32)]
#[derive(PartialEq, Eq, Debug)]
pub enum PdfiumError {
    /// Unknown error.
    Unknown = pdfium_bindings::FPDF_ERR_UNKNOWN as i32,
    /// File not found or could not be opened.
    BadFile = pdfium_bindings::FPDF_ERR_FILE as i32,
    /// File not in PDF format or corrupted.
    BadFormat = pdfium_bindings::FPDF_ERR_FORMAT as i32,
    /// Password required or incorrect password.
    BadPassword = pdfium_bindings::FPDF_ERR_PASSWORD as i32,
    /// Unsupported security scheme.
    UnsupportedSecurityScheme = pdfium_bindings::FPDF_ERR_SECURITY as i32,
    /// Page not found or content error.
    BadPage = pdfium_bindings::FPDF_ERR_PAGE as i32,
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

#[repr(i32)]
#[derive(Debug, PartialEq, Eq)]
pub enum BitmapFormat {
    /// Gray scale bitmap, one byte per pixel.
    GreyScale = pdfium_bindings::FPDFBitmap_Gray as i32,
    /// 3 bytes per pixel, byte order: blue, green, red.
    BGR = pdfium_bindings::FPDFBitmap_BGR as i32,
    /// 4 bytes per pixel, byte order: blue, green, red, unused.
    BGRx = pdfium_bindings::FPDFBitmap_BGRx as i32,
    /// 4 bytes per pixel, byte order: blue, green, red, alpha.
    BGRA = pdfium_bindings::FPDFBitmap_BGRA as i32,
}

impl BitmapFormat {
    /// Number of bytes per pixel.
    ///
    /// Useful when creating an external bitmap or when indexing into a bitmap's buffer.
    /// ## Example
    /// ```
    /// use pdfium_core::BitmapFormat;
    ///
    /// assert_eq!(BitmapFormat::BGRA.bytes_per_pixel(), 4);
    /// ```
    pub fn bytes_per_pixel(&self) -> usize {
        match *self {
            BitmapFormat::GreyScale => 1,
            BitmapFormat::BGR => 3,
            BitmapFormat::BGRx | BitmapFormat::BGRA => 4,
        }
    }

    fn from_i32(number: i32) -> Option<BitmapFormat> {
        match number {
            x if x == BitmapFormat::GreyScale as i32 => Some(BitmapFormat::GreyScale),
            x if x == BitmapFormat::BGR as i32 => Some(BitmapFormat::BGR),
            x if x == BitmapFormat::BGRx as i32 => Some(BitmapFormat::BGRx),
            x if x == BitmapFormat::BGRA as i32 => Some(BitmapFormat::BGRA),
            _ => None,
        }
    }
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

pub mod rendering_flags {
    //! Page rendering flags used for [`render_page_to_bitmap`](crate::Library::render_page_to_bitmap). They can be combined with bit-wise OR.
    //!
    //! ## Examples
    //! ```
    //! use pdfium_core::rendering_flags::*;
    //!
    //! // Set flags for gray scale and printing
    //! let flags = GRAY_SCALE | PRINTING;
    //! ```

    /// Normal display (No flags)
    pub const NORMAL: i32 = 0;

    /// Set if annotations are to be rendered.
    pub const ANNOTATIONS: i32 = pdfium_bindings::FPDF_ANNOT as i32;

    /// Set if using text rendering optimized for LCD display. This flag will only
    /// take effect if anti-aliasing is enabled for text.
    pub const LCD_TEXT: i32 = pdfium_bindings::FPDF_LCD_TEXT as i32;

    /// Don't use the native text output available on some platforms
    pub const NO_NATIVE_TEXT: i32 = pdfium_bindings::FPDF_NO_NATIVETEXT as i32;

    /// Grayscale output
    pub const GRAY_SCALE: i32 = pdfium_bindings::FPDF_GRAYSCALE as i32;

    /// Limit image cache size.
    pub const LIMITED_IMAGE_CACHE: i32 = pdfium_bindings::FPDF_RENDER_LIMITEDIMAGECACHE as i32;

    /// Always use halftone for image stretching.
    pub const FORCE_HALFTONE: i32 = pdfium_bindings::FPDF_RENDER_FORCEHALFTONE as i32;

    /// Render for printing.
    pub const PRINTING: i32 = pdfium_bindings::FPDF_PRINTING as i32;

    /// Set to disable anti-aliasing on text. This flag will also disable LCD
    /// optimization for text rendering.
    pub const NO_SMOOTH_TEXT: i32 = pdfium_bindings::FPDF_RENDER_NO_SMOOTHTEXT as i32;

    /// Set to disable anti-aliasing on images.
    pub const NO_SMOOTH_IMAGE: i32 = pdfium_bindings::FPDF_RENDER_NO_SMOOTHIMAGE as i32;

    /// Set to disable anti-aliasing on paths.
    pub const NO_SMOOTH_PATH: i32 = pdfium_bindings::FPDF_RENDER_NO_SMOOTHPATH as i32;

    /// Set whether to render in a reverse Byte order, this flag is only used when
    /// rendering to a bitmap.
    pub const REVERSE_BYTE_ORDER: i32 = pdfium_bindings::FPDF_REVERSE_BYTE_ORDER as i32;
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

impl<'a> fmt::Debug for DocumentHandle<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DocumentHandle")
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

use std::ffi::CString;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[cfg(not(unix))]
fn cstr(path: &Path) -> Result<CString, PdfiumError> {
    let path = path.to_str().ok_or(PdfiumError::BadFile)?;
    CString::new(path).map_err(|_| PdfiumError::BadFile)
}

#[cfg(unix)]
fn cstr(path: &Path) -> Result<CString, PdfiumError> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| PdfiumError::BadFile)
}

#[cfg(test)]
use parking_lot::{const_mutex, Mutex};

#[cfg(test)]
static TEST_LOCK: Mutex<()> = const_mutex(());

#[cfg(test)]
mod tests {
    use super::*;

    use std::ffi::CString;

    static DUMMY_PDF: &'static [u8] = include_bytes!("../../../test_assets/dummy.pdf");
    static DUMMY_PASSWORD_PDF: &'static [u8] = include_bytes!("../../../test_assets/password.pdf");

    #[test]
    fn only_one_library_at_a_time() {
        let _guard = TEST_LOCK.lock();
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
        let _guard = TEST_LOCK.lock();
        let mut library = Library::init_library().unwrap();
        let document = library.load_mem_document(DUMMY_PDF, None).unwrap();

        assert_eq!(library.get_page_count(&document), 1);
    }

    #[test]
    fn page_dimensions() {
        let _guard = TEST_LOCK.lock();
        let mut library = Library::init_library().unwrap();
        let document = library.load_mem_document(DUMMY_PDF, None).unwrap();
        let page = library.load_page(&document, 0).unwrap();

        assert_eq!(library.get_page_width(&page), 595.0);
        assert_eq!(library.get_page_height(&page), 842.0);
    }

    #[test]
    fn render() {
        let _guard = TEST_LOCK.lock();
        let mut library = Library::init_library().unwrap();
        let document = library.load_mem_document(DUMMY_PDF, None).unwrap();
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

        library.render_page_to_bitmap(
            &mut bitmap,
            &page,
            0,
            0,
            width as i32,
            height as i32,
            PageOrientation::Normal,
            rendering_flags::NORMAL,
        );

        drop(bitmap);

        // There is at least one none white pixel
        assert!(buffer.iter().any(|x| *x != 0xFF));
    }

    mod load_mem_document {
        use super::*;

        #[test]
        fn no_password() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let document_handle = library.load_mem_document(DUMMY_PDF, None);

            assert!(document_handle.is_ok());
        }

        #[test]
        fn password() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let password = CString::new("test").unwrap();
            let document_handle = library.load_mem_document(DUMMY_PASSWORD_PDF, Some(&password));
            assert!(document_handle.is_ok());
        }

        #[test]
        fn bad_password() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let password = CString::new("wrong password").unwrap();
            let document_handle = library.load_mem_document(DUMMY_PASSWORD_PDF, Some(&password));
            assert_eq!(document_handle.unwrap_err(), PdfiumError::BadPassword);
        }

        #[test]
        fn password_missing() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let document_handle = library.load_mem_document(DUMMY_PASSWORD_PDF, None);
            assert_eq!(document_handle.unwrap_err(), PdfiumError::BadPassword);
        }

        #[test]
        fn unneeded_password() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let password = CString::new("wrong password").unwrap();
            let document_handle = library.load_mem_document(DUMMY_PDF, Some(&password));
            assert!(document_handle.is_ok());
        }

        #[test]
        fn no_data() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let document_handle = library.load_mem_document(&[], None);
            assert_eq!(document_handle.unwrap_err(), PdfiumError::BadFormat);
        }

        #[test]
        fn bad_data() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let document_handle = library.load_mem_document(&[0; 255], None);
            assert_eq!(document_handle.unwrap_err(), PdfiumError::BadFormat);
        }
    }

    mod load_document {
        use super::*;
        use std::path::{Path, PathBuf};

        fn test_assets_path() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("test_assets")
        }

        fn test_assert(filename: &str) -> PathBuf {
            test_assets_path().join(filename)
        }

        fn dummy_pdf_path() -> PathBuf {
            test_assert("dummy.pdf")
        }

        fn dummy_password_pdf_path() -> PathBuf {
            test_assert("password.pdf")
        }

        #[test]
        fn no_password() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let document_handle = library.load_document(&dummy_pdf_path(), None);

            assert!(document_handle.is_ok());
        }

        #[test]
        fn password() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let password = CString::new("test").unwrap();
            let document_handle =
                library.load_document(&dummy_password_pdf_path(), Some(&password));
            assert!(document_handle.is_ok());
        }

        #[test]
        fn bad_password() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let password = CString::new("wrong password").unwrap();
            let document_handle =
                library.load_document(&dummy_password_pdf_path(), Some(&password));
            assert_eq!(document_handle.unwrap_err(), PdfiumError::BadPassword);
        }

        #[test]
        fn password_missing() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let document_handle = library.load_document(&dummy_password_pdf_path(), None);
            assert_eq!(document_handle.unwrap_err(), PdfiumError::BadPassword);
        }

        #[test]
        fn unneeded_password() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let password = CString::new("wrong password").unwrap();
            let document_handle = library.load_document(&dummy_pdf_path(), Some(&password));
            assert!(document_handle.is_ok());
        }

        #[test]
        fn no_data() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let document_handle = library.load_document(&test_assert("empty.pdf"), None);
            assert_eq!(document_handle.unwrap_err(), PdfiumError::BadFormat);
        }

        #[test]
        fn bad_data() {
            let _guard = TEST_LOCK.lock();
            let mut library = Library::init_library().unwrap();
            let document_handle = library.load_document(&test_assert("bad.pdf"), None);
            assert_eq!(document_handle.unwrap_err(), PdfiumError::BadFormat);
        }
    }
}
