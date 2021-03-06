# PDFium Core
A safe, minimal Rust wrapper around the PDFium library.

## Install PDFium

This crate loads PDFium as a binary library and also uses the headers from the system so it most be installed in order to use this crate.

Download the prebuilt PDFium binary from: https://github.com/bblanchon/pdfium-binaries.
This crate doesn't use any the V8 or XFA features from PDFium so you only have to use the base library.

## Road Map for pdfium_core

- 🚀 MVP (can render an image)
- 🚀 Prevent misuse (Ensure synchronization between threads)
- 🚀 Documentation
- 🚀 Add missing APIs
- 🚀 Examples
- 🚀 Improve Tests
- 🚀 Choose Licence
- Finish README
- Get code review
- Publish on crates.io
- Add CI