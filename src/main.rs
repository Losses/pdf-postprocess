use log::info;
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::env;
use std::fs::remove_file;
use std::path::{Path, PathBuf};
use tracing_subscriber::filter::EnvFilter;
use walkdir::WalkDir;

fn render_svg_to_pdf(svg_path: &str, pdf_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use librsvg_rebind::prelude::HandleExt;

    // Set the environment variable to allow huge XML files
    env::set_var("XML_PARSE_HUGE", "1");

    // Load the SVG file
    let handle = librsvg_rebind::Handle::from_file(svg_path)?.ok_or("Failed to load SVG file")?;

    // Get the intrinsic size of the SVG in pixels
    let (width, height) = handle
        .intrinsic_size_in_pixels()
        .ok_or("Failed to get intrinsic size")?;

    // Create a PDF surface
    let pdf_surface = cairo::PdfSurface::new(width as f64, height as f64, pdf_path)?;
    let context = cairo::Context::new(&pdf_surface)?;

    // Define the viewport for rendering
    let viewport = librsvg_rebind::Rectangle::new(0., 0., width as f64, height as f64);

    // Render the SVG onto the PDF surface
    handle.render_document(&context, &viewport)?;

    // Finish the PDF surface to ensure all data is written
    pdf_surface.finish();

    Ok(())
}

fn merge_pdfs(
    output_files: Vec<PathBuf>,
    merged_output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut max_id = 1;
    let mut pagenum = 1;
    let mut documents_pages = BTreeMap::new();
    let mut documents_objects = BTreeMap::new();
    let mut document = Document::with_version("1.5");

    for output_file in output_files {
        let mut doc = Document::load(output_file)?;
        let mut first = false;
        doc.renumber_objects_with(max_id);

        max_id = doc.max_id + 1;

        documents_pages.extend(
            doc.get_pages()
                .into_values()
                .map(|object_id| {
                    if !first {
                        let bookmark = lopdf::Bookmark::new(
                            format!("Page_{}", pagenum),
                            [0.0, 0.0, 1.0],
                            0,
                            object_id,
                        );
                        document.add_bookmark(bookmark, None);
                        first = true;
                        pagenum += 1;
                    }

                    (object_id, doc.get_object(object_id).unwrap().to_owned())
                })
                .collect::<BTreeMap<ObjectId, Object>>(),
        );
        documents_objects.extend(doc.objects);
    }

    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    for (object_id, object) in documents_objects.iter() {
        match object.type_name().unwrap_or("") {
            "Catalog" => {
                catalog_object = Some((
                    if let Some((id, _)) = catalog_object {
                        id
                    } else {
                        *object_id
                    },
                    object.clone(),
                ));
            }
            "Pages" => {
                if let Ok(dictionary) = object.as_dict() {
                    let mut dictionary = dictionary.clone();
                    if let Some((_, ref object)) = pages_object {
                        if let Ok(old_dictionary) = object.as_dict() {
                            dictionary.extend(old_dictionary);
                        }
                    }

                    pages_object = Some((
                        if let Some((id, _)) = pages_object {
                            id
                        } else {
                            *object_id
                        },
                        Object::Dictionary(dictionary),
                    ));
                }
            }
            "Page" => {}
            "Outlines" => {}
            "Outline" => {}
            _ => {
                document.objects.insert(*object_id, object.clone());
            }
        }
    }

    if pages_object.is_none() {
        println!("Pages root not found.");
        return Ok(());
    }

    for (object_id, object) in documents_pages.iter() {
        if let Ok(dictionary) = object.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Parent", pages_object.as_ref().unwrap().0);

            document
                .objects
                .insert(*object_id, Object::Dictionary(dictionary));
        }
    }

    if catalog_object.is_none() {
        println!("Catalog root not found.");
        return Ok(());
    }

    let catalog_object = catalog_object.unwrap();
    let pages_object = pages_object.unwrap();

    if let Ok(dictionary) = pages_object.1.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Count", documents_pages.len() as u32);
        dictionary.set(
            "Kids",
            documents_pages
                .into_keys()
                .map(Object::Reference)
                .collect::<Vec<_>>(),
        );

        document
            .objects
            .insert(pages_object.0, Object::Dictionary(dictionary));
    }

    if let Ok(dictionary) = catalog_object.1.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Pages", pages_object.0);
        dictionary.remove(b"Outlines");

        document
            .objects
            .insert(catalog_object.0, Object::Dictionary(dictionary));
    }

    document.trailer.set("Root", catalog_object.0);
    document.max_id = document.objects.len() as u32;
    document.renumber_objects();
    document.adjust_zero_pages();

    if let Some(n) = document.build_outline() {
        if let Ok(Object::Dictionary(ref mut dict)) = document.get_object_mut(catalog_object.0) {
            dict.set("Outlines", Object::Reference(n));
        }
    }

    document.compress();
    document.save(merged_output_path)?;

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

    let mut output_files = Vec::new();

    for entry in WalkDir::new(&svg_dir).into_iter().filter_map(Result::ok) {
        if entry.path().extension().and_then(|s| s.to_str()) == Some("svg") {
            let svg_path = entry.path();
            let file_name = svg_path.file_stem().unwrap().to_str().unwrap();
            let output_path = PathBuf::from(&svg_dir).join(format!("{}.pdf", file_name));

            render_svg_to_pdf(svg_path.to_str().unwrap(), output_path.to_str().unwrap())?;
            output_files.push(output_path);
        }
    }

    output_files.sort();

    info!("Merging all files into a single report");
    let merged_output_path = PathBuf::from(&svg_dir).join("merged.pdf");
    merge_pdfs(output_files.clone(), &merged_output_path)?;

    for output_file in output_files {
        info!("Cleaning file:{:?}", &output_file);
        remove_file(output_file)?;
    }

    Ok(())
}
