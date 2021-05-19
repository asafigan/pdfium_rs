//! The PDFium library is not thread safe so all types in `pdfium_rs` are `!Send` and `!Sync`.
//! Trying to use these types across threads will not compile.
//!
//! Example:
//! ```compile_fail
//! use pdfium_rs::Library;
//!
//! let mut library = Library::init().unwrap();
//!
//! // Fails to compile because Library is !Send.
//! std::thread::spawn(move || {
//!     let document = library.document_from_bytes(&[]);
//! });
//! ```
//!
//! ## Install PDFium
//! This crate loads PDFium as a binary library and also uses the headers from the system so it most be installed in order to use this crate.
//!
//! Download the prebuilt PDFium binary from: https://github.com/bblanchon/pdfium-binaries.
//!
//! This crate doesn't use any the V8 or XFA features from PDFium so you only have to use the base library.

#![forbid(unsafe_code)]

pub use pdfium_core::{BitmapFormat, PageOrientation, PdfiumError};

pub struct Library {
    core: pdfium_core::Library,
}

impl Library {
    pub fn init() -> Option<Library> {
        pdfium_core::Library::init_library().map(|library| Library { core: library })
    }

    pub fn document_from_bytes<'a>(
        &'a self,
        buffer: &'a [u8],
    ) -> Result<Document<'a, 'a>, PdfiumError> {
        let handle = self.core.load_document_from_bytes(buffer, None);

        handle.map(|handle| Document {
            handle,
            core: &self.core,
        })
    }

    pub fn bitmap_from_external_buffer<'a>(
        &'a self,
        width: usize,
        height: usize,
        height_stride: usize,
        format: BitmapFormat,
        buffer: &'a mut [u8],
    ) -> Result<Bitmap<'a, 'a>, PdfiumError> {
        let handle =
            self.core
                .create_bitmap_from_buffer(width, height, format, buffer, height_stride);

        handle.map(|handle| Bitmap {
            handle,
            core: &self.core,
        })
    }
}

pub struct Document<'data, 'library> {
    handle: pdfium_core::DocumentHandle<'data, 'library>,
    core: &'library pdfium_core::Library,
}

impl Document<'_, '_> {
    pub fn page_count(&self) -> usize {
        self.core.get_page_count(&self.handle)
    }

    pub fn page(&self, index: usize) -> Result<Page, PdfiumError> {
        let handle = self.core.load_page(&self.handle, index);

        handle.map(|handle| Page {
            handle,
            core: self.core,
        })
    }
}

pub struct Page<'data, 'library> {
    handle: pdfium_core::PageHandle<'data, 'library>,
    core: &'library pdfium_core::Library,
}

impl Page<'_, '_> {
    pub fn width(&self) -> f32 {
        self.core.get_page_width(&self.handle)
    }

    pub fn height(&self) -> f32 {
        self.core.get_page_height(&self.handle)
    }

    pub fn render_to(&self, bitmap: &mut Bitmap) {
        let width = bitmap.width() as i32;
        let height = bitmap.height() as i32;
        self.core.render_page_to_bitmap(
            &mut bitmap.handle,
            &self.handle,
            0,
            0,
            width,
            height,
            PageOrientation::Normal,
            0,
        );
    }
}

pub struct Bitmap<'data, 'library> {
    handle: pdfium_core::BitmapHandle<'data, 'library>,
    core: &'library pdfium_core::Library,
}

impl Bitmap<'_, '_> {
    pub fn width(&self) -> usize {
        self.core.get_bitmap_width(&self.handle)
    }

    pub fn height(&self) -> usize {
        self.core.get_bitmap_height(&self.handle)
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: u64) {
        self.core
            .bitmap_fill_rect(&mut self.handle, x, y, width, height, color)
    }
}

#[cfg(test)]
use parking_lot::{const_mutex, Mutex};

#[cfg(test)]
static TEST_LOCK: Mutex<()> = const_mutex(());

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Bgra, ImageBuffer};

    static DUMMY_PDF: &'static [u8] = include_bytes!("../test_assets/dummy.pdf");

    #[test]
    fn only_one_library_at_a_time() {
        let _guard = TEST_LOCK.lock();
        let first = Library::init();
        assert!(first.is_some());
        let second = Library::init();
        assert!(second.is_none());

        drop(first);
        let third = Library::init();
        assert!(third.is_some());
    }

    #[test]
    fn page_count() {
        let _guard = TEST_LOCK.lock();
        let library = Library::init().unwrap();
        let document = library.document_from_bytes(DUMMY_PDF).unwrap();

        assert_eq!(document.page_count(), 1);
    }

    #[test]
    fn page_dimensions() {
        let _guard = TEST_LOCK.lock();
        let library = Library::init().unwrap();
        let document = library.document_from_bytes(DUMMY_PDF).unwrap();
        let page = document.page(0).unwrap();

        assert_eq!(page.width(), 595.0);
        assert_eq!(page.height(), 842.0);
    }

    #[test]
    fn render() {
        let _guard = TEST_LOCK.lock();
        let library = Library::init().unwrap();
        let document = library.document_from_bytes(DUMMY_PDF).unwrap();
        let page = document.page(0).unwrap();

        // Create white image
        let mut image = ImageBuffer::from_pixel(
            page.width().round() as u32,
            page.height().round() as u32,
            Bgra::<u8>([0xFF; 4]),
        );
        let layout = image.sample_layout();
        let (width, height) = image.dimensions();
        let mut buffer = image.as_flat_samples_mut();
        let buffer = buffer.image_mut_slice().unwrap();

        let mut bitmap = library
            .bitmap_from_external_buffer(
                width as usize,
                height as usize,
                layout.height_stride,
                BitmapFormat::BGRA,
                buffer,
            )
            .unwrap();

        page.render_to(&mut bitmap);

        drop(bitmap);

        // There is at least one none white pixel
        assert!(image.pixels().any(|x| *x != Bgra::<u8>([0xFF; 4])));
    }
}
