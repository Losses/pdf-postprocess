# PDF-postprocess

This project aims to address the limitations of typst in handling certain special styles, such as SVG files with unique formats or gradients with transparency. By exporting your typst document to SVG format and then using this program, you can convert the SVG files to PDFs that support these special formats using the librsvg library.

## Features

- Converts SVG files to PDFs while retaining special styles.
- Merges multiple SVG files into a single document.
- Utilizes the librsvg library for rendering.

## Requirements

- Rust (for building the project)
- librsvg installed on the system

## Usage

1. **Export your Typst document to SVG format.**

   ```bash
    typst compile main.typ -f svg /path/to/output/{n}.svg
   ```

2. **Run the converter:**

   ```bash
   ./target/release/pdf-postprocess <svg_directory>
   ```

   - `<svg_directory>`: The directory containing your SVG files.

## Example

```bash
./target/release/pdf-postprocess ./svg-files
```

This command will convert all SVG files in the `./svg-files` directory to PDFs and save them in the same directory. Additionally, it will merge all the PDFs into a single file named `merged.pdf` in the same directory.

## Updates

### Version 0.2.0

- **Migrated to librsvg**: The rendering process has been updated to use the librsvg library instead of the Chromium browser. This change makes the rendering process more lightweight and efficient.
- **Base64 SVG Expansion**: Added functionality to decode base64-encoded SVG images embedded within the main SVG file to prevent common bugs happens on librsvg.

## License

This project is licensed under the MIT License.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request for any improvements or bug fixes.

## Acknowledgments

- [librsvg](https://gnome.pages.gitlab.gnome.org/librsvg/Rsvg-2.0/index.html) for SVG rendering.
- [lopdf](https://docs.rs/lopdf/latest/lopdf/) for PDF manipulation.
- [walkdir](https://docs.rs/walkdir/latest/walkdir/) for directory traversal.
