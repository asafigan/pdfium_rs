use std::ffi::{c_void, CString, NulError};
use std::marker::PhantomData;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

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

    pub fn get_last_error(&mut self) -> u64 {
        unsafe { pdfium_bindings::FPDF_GetLastError() }
    }

    pub fn load_document(
        &mut self,
        path: &Path,
        password: impl Into<Vec<u8>>,
    ) -> Result<Option<DocumentHandle<'static>>, NulError> {
        let path = CString::new(path.to_string_lossy().to_string().into_bytes())?.as_ptr();
        let password = CString::new(password)?.as_ptr();
        let handle = unsafe { pdfium_bindings::FPDF_LoadDocument(path, password).as_mut() };

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
        let handle = unsafe {
            pdfium_bindings::FPDF_LoadMemDocument(
                buffer.as_ptr() as *mut c_void,
                buffer.len() as i32,
                password,
            )
            .as_mut()
        };

        Ok(handle.map(|handle| DocumentHandle {
            handle,
            life_time: Default::default(),
        }))
    }

    pub fn get_page_count(&mut self, document: &DocumentHandle) -> usize {
        unsafe { pdfium_bindings::FPDF_GetPageCount(document.handle) as usize }
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

        let handle = unsafe {
            pdfium_bindings::FPDFBitmap_CreateEx(
                width as i32,
                height as i32,
                format as i32,
                buffer.as_ptr() as *mut c_void,
                height_stride as i32,
            )
            .as_mut()
        };

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
        let handle =
            unsafe { pdfium_bindings::FPDF_LoadPage(document.handle, index as i32).as_mut() };

        handle.map(|handle| PageHandle {
            handle,
            life_time: Default::default(),
        })
    }

    pub fn get_page_width(&mut self, page: &PageHandle) -> f32 {
        unsafe { pdfium_bindings::FPDF_GetPageWidthF(page.handle) }
    }

    pub fn get_page_height(&mut self, page: &PageHandle) -> f32 {
        unsafe { pdfium_bindings::FPDF_GetPageHeightF(page.handle) }
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
                bitmap.handle,
                page.handle,
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
        unsafe { pdfium_bindings::FPDFBitmap_GetWidth(bitmap.handle) as u32 }
    }

    pub fn get_bitmap_height(&mut self, bitmap: &BitmapHandle) -> u32 {
        unsafe { pdfium_bindings::FPDFBitmap_GetHeight(bitmap.handle) as u32 }
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
        unsafe { pdfium_bindings::FPDFBitmap_FillRect(bitmap.handle, x, y, width, height, color) }
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

pub struct DocumentHandle<'a> {
    handle: pdfium_bindings::FPDF_DOCUMENT,
    life_time: PhantomData<&'a [u8]>,
}

impl<'a> Drop for DocumentHandle<'a> {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDF_CloseDocument(self.handle);
        }
    }
}

pub struct PageHandle<'a> {
    handle: pdfium_bindings::FPDF_PAGE,
    life_time: PhantomData<&'a [u8]>,
}

impl<'a> Drop for PageHandle<'a> {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDF_ClosePage(self.handle);
        }
    }
}

pub struct BitmapHandle<'a> {
    handle: pdfium_bindings::FPDF_BITMAP,
    life_time: PhantomData<&'a mut [u8]>,
}

impl<'a> Drop for BitmapHandle<'a> {
    fn drop(&mut self) {
        unsafe {
            pdfium_bindings::FPDFBitmap_Destroy(self.handle);
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
