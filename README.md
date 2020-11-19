# PDFium RS
Using the PDFium library to render PDFs in Rust.

> ⚠️ PDFium is not a thread safe library. The types in pdfium_rs and pdfium_core will prevent this from causing any issues, but, if you need to render multiple PDFs at the same time, you will have to do it from multiple processes not threads. Providing a rendering pool that does this is on the road map.

## Road Map

- 🚀 MVP (can render an image)
- 🚀 Prevent misuse (Ensure synchronization between threads)
- Documentation
- Examples
- Improve Tests
- Expand API
- Rendering Pool
