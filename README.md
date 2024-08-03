# PDF-postprocess

This project aims to address the limitations of typst in handling certain special styles, such as SVG files with unique formats or gradients with transparency. By exporting your typst document to SVG format and then using this program, you can convert the SVG files to PDFs that support these special formats using the system's Chromium browser.

## Features

- Converts SVG files to PDFs while retaining special styles.
- Merges multiple SVG files into a single document.
- Utilizes the system's Chromium browser for rendering.

## Requirements

- Rust (for building the project)
- Chromium browser installed on the system

## Usage

1. **Export your Typst document to SVG format.**

    ```bash
     typst compile main.typ -f svg /path/to/output/{n}.svg
    ```

2. **Run the converter:**

   ```bash
   ./target/release/pdf-postprocess <svg_directory> <output_directory>
   ```

   - `<svg_directory>`: The directory containing your SVG files.
   - `<output_directory>`: The directory where the converted PDFs will be saved.

## Example

```bash
./target/release/pdf-postprocess ./svg-files ./output-pdfs
```

This command will convert all SVG files in the `./svg-files` directory to PDFs and save them in the `./output-pdfs` directory. Additionally, it will merge all the PDFs into a single file named `merged.pdf` in the output directory.

## License

This project is licensed under the MIT License.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request for any improvements or bug fixes.

## Acknowledgments

- [headless_chrome](https://docs.rs/headless_chrome/latest/headless_chrome/) for Chromium browser automation.
- [lopdf](https://docs.rs/lopdf/latest/lopdf/) for PDF manipulation.
- [walkdir](https://docs.rs/walkdir/latest/walkdir/) for directory traversal.
