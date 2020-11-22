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

pub use pdfium_core::{BitmapFormat, PageOrientation, PdfiumError};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Library {
    core: Rc<RefCell<pdfium_core::Library>>,
}

impl Library {
    pub fn init() -> Option<Library> {
        pdfium_core::Library::init_library().map(|library| Library {
            core: Rc::new(RefCell::new(library)),
        })
    }

    pub fn document_from_bytes<'a>(&self, buffer: &'a [u8]) -> Result<Document<'a>, PdfiumError> {
        let handle = self.core.borrow_mut().load_mem_document(buffer, None);

        handle.map(|handle| Document {
            handle,
            core: self.core.clone(),
        })
    }

    pub fn bitmap_from_external_buffer<'a>(
        &self,
        width: usize,
        height: usize,
        height_stride: usize,
        format: BitmapFormat,
        buffer: &'a mut [u8],
    ) -> Result<Bitmap<'a>, PdfiumError> {
        let handle = self.core.borrow_mut().create_external_bitmap(
            width,
            height,
            format,
            buffer,
            height_stride,
        );

        handle.map(|handle| Bitmap {
            handle,
            core: self.core.clone(),
        })
    }
}

pub struct Document<'a> {
    handle: pdfium_core::DocumentHandle<'a>,
    core: Rc<RefCell<pdfium_core::Library>>,
}

impl<'a> Document<'a> {
    pub fn page_count(&self) -> usize {
        self.core.borrow_mut().get_page_count(&self.handle)
    }

    pub fn page(&self, index: usize) -> Result<Page, PdfiumError> {
        let handle = self.core.borrow_mut().load_page(&self.handle, index);

        handle.map(|handle| Page {
            handle,
            core: self.core.clone(),
        })
    }
}

pub struct Page<'a> {
    handle: pdfium_core::PageHandle<'a>,
    core: Rc<RefCell<pdfium_core::Library>>,
}

impl<'a> Page<'a> {
    pub fn width(&self) -> f32 {
        self.core.borrow_mut().get_page_width(&self.handle)
    }

    pub fn height(&self) -> f32 {
        self.core.borrow_mut().get_page_height(&self.handle)
    }

    pub fn render_to(&self, bitmap: &mut Bitmap) {
        let width = bitmap.width() as i32;
        let height = bitmap.height() as i32;
        self.core.borrow_mut().render_page_bitmap(
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

pub struct Bitmap<'a> {
    handle: pdfium_core::BitmapHandle<'a>,
    core: Rc<RefCell<pdfium_core::Library>>,
}

impl<'a> Bitmap<'a> {
    pub fn width(&self) -> u32 {
        self.core.borrow_mut().get_bitmap_width(&self.handle)
    }

    pub fn height(&self) -> u32 {
        self.core.borrow_mut().get_bitmap_height(&self.handle)
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: u64) {
        self.core
            .borrow_mut()
            .bitmap_fill_rect(&mut self.handle, x, y, width, height, color)
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
    use image::{Bgra, ImageBuffer};

    static DUMMY_PDF: &'static [u8] = include_bytes!("../test_assets/dummy.pdf");

    #[test]
    fn only_one_library_at_a_time() {
        let _guard = TEST_LOCK.lock().unwrap();
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
        let _guard = TEST_LOCK.lock().unwrap();
        let library = Library::init().unwrap();
        let document = library.document_from_bytes(DUMMY_PDF).unwrap();

        assert_eq!(document.page_count(), 1);
    }

    #[test]
    fn page_dimensions() {
        let _guard = TEST_LOCK.lock().unwrap();
        let library = Library::init().unwrap();
        let document = library.document_from_bytes(DUMMY_PDF).unwrap();
        let page = document.page(0).unwrap();

        assert_eq!(page.width(), 595.0);
        assert_eq!(page.height(), 842.0);
    }

    #[test]
    fn render() {
        let _guard = TEST_LOCK.lock().unwrap();
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
