use image::{Bgra, ImageBuffer};
use pdfium_core::{rendering_flags, BitmapFormat, Library, PageOrientation};

static DUMMY_PDF: &'static [u8] = include_bytes!("../../../test_assets/dummy.pdf");

fn main() {
    // initialize library
    let mut library = Library::init_library().unwrap();

    // load document
    let document = library.load_document_from_bytes(DUMMY_PDF, None).unwrap();

    // load page
    let page = library.load_page(&document, 0).unwrap();

    // get width and height of the page
    let width = library.get_page_width(&page);
    let height = library.get_page_height(&page);

    // create white image
    let mut image = ImageBuffer::from_pixel(
        width.round() as u32,
        height.round() as u32,
        Bgra::<u8>([0xFF; 4]),
    );

    // get image's buffer
    let layout = image.sample_layout();
    let (width, height) = image.dimensions();
    let mut buffer = image.as_flat_samples_mut();
    let buffer = buffer.image_mut_slice().unwrap();

    // create bitmap
    let mut bitmap = library
        .create_bitmap_from_buffer(
            width as usize,
            height as usize,
            BitmapFormat::BGRA,
            buffer,
            layout.height_stride,
        )
        .unwrap();

    // render pdf
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

    // drop the bitmap so that you can access the image again
    drop(bitmap);

    // there is at least one none white pixel
    assert!(image.pixels().any(|x| *x != Bgra::<u8>([0xFF; 4])));
}
