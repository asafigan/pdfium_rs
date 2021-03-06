# PDFium RS
Using the PDFium library to render PDFs in Rust.

> ⚠️ PDFium is not a thread safe library. The types in pdfium_rs and pdfium_core will prevent misuse so no need to worry. If you need to render multiple PDFs at the same time, you will have to do it from multiple processes not threads. Providing a rendering pool that does this is on the road map.

## Install PDFium

This crate loads PDFium as a binary library and also uses the headers from the system so it most be installed in order to use this crate.

Download the prebuilt PDFium binary from: https://github.com/bblanchon/pdfium-binaries.
This crate doesn't use any the V8 or XFA features from PDFium so you only have to use the base library.

## Road Map

- 🚀 MVP (can render an image)
- 🚀 Prevent misuse (Ensure synchronization between threads)
- 🚀 Documentation
- 👷‍♂️ Publish pdfium_core
- Add CI
- Expand API
- Examples
- Improve Tests
- Publish on crates.io
- Rendering Pool
