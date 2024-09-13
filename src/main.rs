use std::collections::BTreeMap;
use std::fs::read_to_string;
use std::io::Cursor;
use std::path::PathBuf;
use std::{process, str};

use anyhow::{anyhow, Result};
use base64::Engine;
use log::{error, info};
use lopdf::{Document, Object, ObjectId};
use rayon::prelude::*;
use svg2pdf::{ConversionOptions, PageOptions};
use tracing_subscriber::filter::EnvFilter;
use walkdir::WalkDir;
use xmltree::Element;
use xmltree::EmitterConfig;
use xmltree::XMLNode;

fn expand_base64_svgs(svg_content: &str) -> Result<String> {
    // Parse the SVG content as an XML element
    let mut root: Element = Element::parse(Cursor::new(svg_content))?;

    // Recursively process the XML tree to decode base64 SVG images
    process_element(&mut root).map_err(|e| anyhow::anyhow!(e))?;

    // Convert the modified XML tree back to a string
    let mut output = Vec::new();
    root.write_with_config(&mut output, EmitterConfig::default())?;
    let result = String::from_utf8(output)?;

    Ok(result)
}

fn process_element(element: &mut Element) -> Result<()> {
    // Process all child elements
    for child in &mut element.children {
        if let XMLNode::Element(ref mut child_element) = child {
            process_element(child_element)?;
        }
    }

    // Check if the element is an <image> element with a base64-encoded SVG in the xlink:href attribute
    if element.name == "image" {
        if let Some(href) = element.attributes.get("href") {
            if let Some(base64_data) = href.strip_prefix("data:image/svg+xml;base64,") {
                match base64::prelude::BASE64_STANDARD.decode(base64_data) {
                    Ok(decoded_bytes) => match str::from_utf8(&decoded_bytes) {
                        Ok(decoded_svg) => {
                            // Parse the decoded SVG content as an XML element
                            let decoded_element: Element =
                                Element::parse(Cursor::new(decoded_svg))?;

                            // Create a new <svg> element to wrap the decoded SVG content
                            let mut group_element = Element::new("svg");

                            // Transfer the attributes from the <image> element to the <svg> element
                            for (key, value) in &element.attributes {
                                if key != "xlink:href" && key != "href" {
                                    // Exclude the xlink:href, href attribute
                                    group_element.attributes.insert(key.clone(), value.clone());
                                }
                            }

                            for (key, value) in &decoded_element.attributes {
                                if key != "xmlns" {
                                    // Exclude the xmlns attribute
                                    group_element.attributes.insert(key.clone(), value.clone());
                                }
                            }

                            // Add the decoded SVG content as children of the <svg> element
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

pub fn render_svg_to_pdf(svg_content: &str) -> Result<Vec<u8>> {
    // Expand base64 encoded SVGs
    let expanded_svg_content = expand_base64_svgs(svg_content)?;

    let mut options = svg2pdf::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = svg2pdf::usvg::Tree::from_str(&expanded_svg_content, &options)?;

    let pdf = svg2pdf::to_pdf(&tree, ConversionOptions::default(), PageOptions::default());

    Ok(pdf)
}

pub fn merge_pdfs(output_files: Vec<&[u8]>) -> Result<Document> {
    let mut max_id = 1;
    let mut pagenum = 1;
    let mut documents_pages = BTreeMap::new();
    let mut documents_objects = BTreeMap::new();
    let mut document = Document::with_version("1.5");

    for output_file in output_files {
        let mut doc = Document::load_mem(output_file)?;
        let mut first = false;
        doc.renumber_objects_with(max_id);

        max_id = doc.max_id + 1;

        documents_pages.extend(
            doc.get_pages()
                .into_values()
                .filter_map(|object_id| {
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

                    match doc.get_object(object_id) {
                        Ok(object) => Some((object_id, object.to_owned())),
                        Err(_) => None,
                    }
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
                    catalog_object.map_or(*object_id, |(id, _)| id),
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
                        pages_object.map_or(*object_id, |(id, _)| id),
                        Object::Dictionary(dictionary),
                    ));
                }
            }
            "Page" | "Outlines" | "Outline" => {}
            _ => {
                document.objects.insert(*object_id, object.clone());
            }
        }
    }

    let pages_object = match pages_object {
        Some(pages_object) => pages_object,
        None => {
            return Err(anyhow!("Pages root not found."));
        }
    };

    for (object_id, object) in documents_pages.iter() {
        if let Ok(dictionary) = object.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Parent", pages_object.0);

            document
                .objects
                .insert(*object_id, Object::Dictionary(dictionary));
        }
    }

    let catalog_object = match catalog_object {
        Some(catalog_object) => catalog_object,
        None => {
            return Err(anyhow!("Catalog root not found."));
        }
    };

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

    Ok(document)
}

fn main() -> Result<()> {
    let filter = EnvFilter::new("info");

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .init();

    let svg_dir = std::env::args()
        .nth(1)
        .expect("Please provide a directory path");

    let svg_entries: Vec<_> = WalkDir::new(&svg_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("svg"))
        .collect();

    if svg_entries.is_empty() {
        error!("No pages found.");
        process::exit(1);
    }

    let rendered_pages: Vec<(PathBuf, Vec<u8>)> = svg_entries
        .par_iter()
        .filter_map(|entry| {
            let svg_path = entry.path();
            match read_to_string(svg_path) {
                Ok(svg_content) => match render_svg_to_pdf(&svg_content) {
                    Ok(pdf_data) => {
                        info!("Rendering file: {:?}", &svg_path);
                        Some((svg_path.to_path_buf(), pdf_data))
                    }
                    Err(e) => {
                        error!("Error reading SVG file {:?}: {:?}", svg_path, e);
                        process::exit(1)
                    }
                },
                Err(e) => {
                    error!("Error reading SVG file {:?}: {:?}", svg_path, e);
                    process::exit(1)
                }
            }
        })
        .collect();

    // Sort the output files by their path
    let mut output_files = rendered_pages;
    output_files.sort_by_key(|(path, _)| path.clone());

    info!("Merging all files into a single report");
    let merged_output_path = PathBuf::from(&svg_dir).join("merged.pdf");
    let mut merged_pdf = merge_pdfs(
        output_files
            .iter()
            .map(|(_, data)| data.as_slice())
            .collect(),
    )?;

    match merged_pdf.save(merged_output_path.clone()) {
        Ok(_) => {
            info!("Document converted successfuly.");
        }
        Err(e) => {
            error!("Merging PDF error: {}", e);
            process::exit(1);
        }
    }

    // for (path, _) in output_files {
    //     info!("Cleaning file: {:?}", &path);
    //     remove_file(path)?;
    // }

    Ok(())
}
