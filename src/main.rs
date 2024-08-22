use base64::Engine;
use log::info;
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::remove_file;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::str;
use tracing_subscriber::filter::EnvFilter;
use walkdir::WalkDir;
use xmltree::Element;
use xmltree::EmitterConfig;
use xmltree::XMLNode;

fn expand_base64_svgs(svg_content: &str) -> Result<String, Box<dyn Error>> {
    // Parse the SVG content as an XML element
    let mut root: Element = Element::parse(Cursor::new(svg_content))?;

    // Recursively process the XML tree to decode base64 SVG images
    process_element(&mut root)?;

    // Convert the modified XML tree back to a string
    let mut output = Vec::new();
    root.write_with_config(&mut output, EmitterConfig::default())?;
    let result = String::from_utf8(output)?;

    Ok(result)
}

fn process_element(element: &mut Element) -> Result<(), Box<dyn Error>> {
    // Process all child elements
    for child in &mut element.children {
        if let XMLNode::Element(ref mut child_element) = child {
            process_element(child_element)?;
        }
    }

    // Check if the element is an <image> element with a base64-encoded SVG in the xlink:href attribute
    if element.name == "image" {
        if let Some(href) = element.attributes.get("href") {
            if href.starts_with("data:image/svg+xml;base64,") {
                let base64_data = &href["data:image/svg+xml;base64,".len()..];
                match base64::prelude::BASE64_STANDARD.decode(base64_data) {
                    Ok(decoded_bytes) => match str::from_utf8(&decoded_bytes) {
                        Ok(decoded_svg) => {
                            // Parse the decoded SVG content as an XML element
                            let decoded_element: Element =
                                Element::parse(Cursor::new(decoded_svg))?;

                            // Create a new <g> element to wrap the decoded SVG content
                            let mut group_element = Element::new("g");

                            // Transfer the attributes from the <image> element to the <g> element
                            for (key, value) in &element.attributes {
                                if key != "xlink:href" && key != "href" {
                                    // Exclude the xlink:href attribute
                                    group_element.attributes.insert(key.clone(), value.clone());
                                }
                            }

                            // Add the decoded SVG content as children of the <g> element
                            for child in decoded_element.children {
                                group_element.children.push(child);
                            }

                            // Replace the <image> element with the group_element SVG content
                            *element = group_element;
                        }
                        Err(_) => {
                            // Handle UTF-8 error, keep the original
                        }
                    },
                    Err(_) => {
                        // Handle base64 decode error, keep the original
                    }
                }
            }
        }
    }

    Ok(())
}

fn render_svg_to_pdf(svg_path: &str, pdf_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use gio::File;
    use librsvg_rebind::prelude::HandleExt;
    use librsvg_rebind::{Handle, HandleFlags};
    use std::fs;

    info!("Reading SVG File: {}", svg_path);

    // Read the SVG file content
    let svg_content = fs::read_to_string(svg_path)?;
    // Expand base64 encoded SVGs
    let expanded_svg_content = expand_base64_svgs(&svg_content)?;

    // Write the expanded SVG content back to a temporary file
    let expanded_svg_path = format!("{}_expanded.svg", svg_path);
    fs::write(&expanded_svg_path, expanded_svg_content)?;

    // Set the required flags
    let flags = HandleFlags::FLAG_UNLIMITED | HandleFlags::FLAG_KEEP_IMAGE_DATA;

    // Create a GFile from the file path
    let file = File::for_path(&expanded_svg_path);
    let handle = match Handle::from_gfile_sync(&file, flags, None::<&gio::Cancellable>) {
        Ok(Some(handle)) => handle,
        Ok(None) => {
            return Err("The SVG file is empty or could not be read.".into());
        }
        Err(e) => {
            return Err(format!("Failed to read the SVG file: {:?}", e).into());
        }
    };

    // Get the intrinsic size of the SVG in pixels
    let (width, height) = handle
        .intrinsic_size_in_pixels()
        .ok_or("Failed to get intrinsic size")?;

    // Create a PDF surface
    let pdf_surface = cairo::PdfSurface::new(width as f64, height as f64, pdf_path)?;
    let pdf_context = cairo::Context::new(&pdf_surface).unwrap();
    // Define the viewport for rendering
    let pdf_viewport = librsvg_rebind::Rectangle::new(0., 0., width as f64, height as f64);
    // Render the SVG onto the PDF surface
    handle.render_document(&pdf_context, &pdf_viewport)?;
    // Finish the PDF surface to ensure all data is written
    pdf_surface.finish();

    // Remove the temporary expanded SVG file
    fs::remove_file(&expanded_svg_path)?;

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
        info!("Cleaning file: {:?}", &output_file);
        remove_file(output_file)?;
    }

    Ok(())
}
