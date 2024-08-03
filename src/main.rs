use headless_chrome::types::PrintToPdfOptions;
use headless_chrome::{Browser, LaunchOptions, Tab};
use log::info;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing_subscriber::filter::EnvFilter;
use walkdir::WalkDir;

fn convert_svg_to_pdf(
    tab: &Tab,
    svg_path: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let svg_path_str = format!("file://{}", svg_path.to_str().unwrap());
    info!("Printing {}", svg_path_str);
    tab.navigate_to(&svg_path_str)?;
    tab.wait_until_navigated()?;

    let pdf_options = PrintToPdfOptions {
        landscape: Some(false),
        display_header_footer: Some(false),
        print_background: Some(true),
        scale: Some(1.0),
        paper_width: Some(8.27),   // A4 size in inches
        paper_height: Some(11.69), // A4 size in inches
        margin_top: Some(0.0),
        margin_bottom: Some(0.0),
        margin_left: Some(0.0),
        margin_right: Some(0.0),
        page_ranges: None,
        ignore_invalid_page_ranges: Some(false),
        header_template: None,
        footer_template: None,
        prefer_css_page_size: Some(true),
        transfer_mode: None,
    };

    let pdf_data = tab.print_to_pdf(Some(pdf_options))?;
    let mut file = File::create(output_path)?;
    file.write_all(&pdf_data)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::new("info");

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .init();

    let svg_dir = std::env::args()
        .nth(1)
        .expect("Please provide a directory path");
    let output_dir = std::env::args()
        .nth(2)
        .expect("Please provide an output directory path");

    let args = vec![OsStr::new("--headless")];
    let browser = Browser::new(
        LaunchOptions::default_builder()
            .headless(true)
            .args(args)
            .build()
            .unwrap(),
    )?;
    let tab = browser.new_tab()?;

    for entry in WalkDir::new(&svg_dir).into_iter().filter_map(Result::ok) {
        if entry.path().extension().and_then(|s| s.to_str()) == Some("svg") {
            let svg_path = entry.path();
            let file_name = svg_path.file_stem().unwrap().to_str().unwrap();
            let output_path = PathBuf::from(&output_dir).join(format!("{}.pdf", file_name));

            convert_svg_to_pdf(&tab, svg_path, &output_path)?;
        }
    }

    Ok(())
}
