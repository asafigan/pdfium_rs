use std::ffi::{c_void, CString, NulError};
use std::marker::PhantomData;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct PDFium(PhantomData<()>);

static INITIALIZED: AtomicBool = AtomicBool::new(false);

impl Drop for PDFium {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDF_DestroyLibrary();
        }
        INITIALIZED.store(false, Ordering::Relaxed);
    }
}

impl PDFium {
    pub fn init() -> Option<PDFium> {
        let already_initialized = INITIALIZED.compare_and_swap(false, true, Ordering::SeqCst);

        if already_initialized {
            None
        } else {
            unsafe {
                pdfium_bindings::FPDF_InitLibrary();
            }
            Some(PDFium(Default::default()))
        }
    }

    pub fn get_last_error(&self) -> u64 {
        unsafe { pdfium_bindings::FPDF_GetLastError() }
    }

    pub fn load_document<'a>(&'a self, path: &Path) -> Result<Option<Document<'a>>, NulError> {
        let path = CString::new(path.to_string_lossy().to_string().into_bytes())?.as_ptr();
        let handle = unsafe { pdfium_bindings::FPDF_LoadDocument(path, std::ptr::null()).as_mut() };

        Ok(handle.map(|handle| Document {
            handle,
            life_time: Default::default(),
        }))
    }

    pub fn load_document_with_password<'a>(
        &'a self,
        path: &Path,
        password: impl Into<Vec<u8>>,
    ) -> Result<Option<Document<'a>>, NulError> {
        let path = CString::new(path.to_string_lossy().to_string().into_bytes())?.as_ptr();
        let password = CString::new(password)?.as_ptr();
        let handle = unsafe { pdfium_bindings::FPDF_LoadDocument(path, password).as_mut() };

        Ok(handle.map(|handle| Document {
            handle,
            life_time: Default::default(),
        }))
    }

    pub fn document_from_bytes<'a>(&'a self, buffer: &'a [u8]) -> Option<Document<'a>> {
        let handle = unsafe {
            pdfium_bindings::FPDF_LoadMemDocument(
                buffer.as_ptr() as *mut c_void,
                buffer.len() as i32,
                std::ptr::null(),
            )
            .as_mut()
        };

        handle.map(|handle| Document {
            handle,
            life_time: Default::default(),
        })
    }

    pub fn bitmap_from_external_buffer<'a>(
        &'a self,
        width: u32,
        height: u32,
        height_stride: usize,
        buffer: &'a mut [u8],
    ) -> Option<Bitmap<'a>> {
        if buffer.len() < (height as usize) * height_stride {
            return None;
        }

        let handle = unsafe {
            pdfium_bindings::FPDFBitmap_CreateEx(
                width as i32,
                height as i32,
                pdfium_bindings::FPDFBitmap_BGRA as i32,
                buffer.as_ptr() as *mut c_void,
                height_stride as i32,
            )
            .as_mut()
        };

        handle.map(|handle| Bitmap {
            handle,
            life_time: Default::default(),
        })
    }
}

pub struct Document<'a> {
    handle: pdfium_bindings::FPDF_DOCUMENT,
    life_time: PhantomData<&'a [u8]>,
}

impl<'a> Drop for Document<'a> {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDF_CloseDocument(self.handle);
        }
    }
}

impl<'a> Document<'a> {
    pub fn page_count(&self) -> usize {
        unsafe { pdfium_bindings::FPDF_GetPageCount(self.handle) as usize }
    }

    pub fn page(&self, index: usize) -> Option<Page> {
        let handle = unsafe { pdfium_bindings::FPDF_LoadPage(self.handle, index as i32).as_mut() };

        handle.map(|handle| Page {
            handle,
            life_time: Default::default(),
        })
    }
}

pub struct Page<'a> {
    handle: pdfium_bindings::FPDF_PAGE,
    life_time: PhantomData<&'a [u8]>,
}

impl<'a> Drop for Page<'a> {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDF_ClosePage(self.handle);
        }
    }
}

impl<'a> Page<'a> {
    pub fn width(&self) -> f32 {
        unsafe { pdfium_bindings::FPDF_GetPageWidthF(self.handle) }
    }

    pub fn height(&self) -> f32 {
        unsafe { pdfium_bindings::FPDF_GetPageHeightF(self.handle) }
    }

    pub fn render_to(&self, bitmap: &mut Bitmap) {
        dbg!(
            bitmap.handle,
            self.handle,
            0,
            0,
            bitmap.width() as i32,
            bitmap.height() as i32,
            0,
            0
        );
        unsafe {
            pdfium_bindings::FPDF_RenderPageBitmap(
                bitmap.handle,
                self.handle,
                0,
                0,
                bitmap.width() as i32,
                bitmap.height() as i32,
                0,
                0,
            );
        }
    }
}

pub struct Bitmap<'a> {
    handle: pdfium_bindings::FPDF_BITMAP,
    life_time: PhantomData<&'a mut [u8]>,
}

impl<'a> Drop for Bitmap<'a> {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDFBitmap_Destroy(self.handle);
        }
    }
}

impl<'a> Bitmap<'a> {
    pub fn width(&self) -> u32 {
        unsafe { pdfium_bindings::FPDFBitmap_GetWidth(self.handle) as u32 }
    }

    pub fn height(&self) -> u32 {
        unsafe { pdfium_bindings::FPDFBitmap_GetHeight(self.handle) as u32 }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: u64) {
        unsafe { pdfium_bindings::FPDFBitmap_FillRect(self.handle, x, y, width, height, color) }
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
    use image::{Bgra, DynamicImage, ImageBuffer};

    static DUMMY_PDF: &'static [u8] = include_bytes!("../test_assets/dummy.pdf");

    #[test]
    fn only_one_library_at_a_time() {
        let _guard = TEST_LOCK.lock().unwrap();
        let first = PDFium::init();
        assert!(first.is_some());
        let second = PDFium::init();
        assert!(second.is_none());

        drop(first);
        let third = PDFium::init();
        assert!(third.is_some());
    }

    #[test]
    fn page_count() {
        let _guard = TEST_LOCK.lock().unwrap();
        let library = PDFium::init().unwrap();
        let document = library.document_from_bytes(DUMMY_PDF).unwrap();

        assert_eq!(document.page_count(), 1);
    }

    #[test]
    fn page_dimensions() {
        let _guard = TEST_LOCK.lock().unwrap();
        let library = PDFium::init().unwrap();
        let document = library.document_from_bytes(DUMMY_PDF).unwrap();
        let page = document.page(0).unwrap();

        assert_eq!(page.width(), 595.0);
        assert_eq!(page.height(), 842.0);
    }

    #[test]
    fn render() {
        let _guard = TEST_LOCK.lock().unwrap();
        let library = PDFium::init().unwrap();
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
            .bitmap_from_external_buffer(width, height, layout.height_stride, buffer)
            .unwrap();

        page.render_to(&mut bitmap);

        assert_eq!(library.get_last_error(), 0);

        drop(bitmap);

        // There is at least one none white pixel
        assert!(image.pixels().any(|x| *x != Bgra::<u8>([0xFF; 4])));
    }
}
