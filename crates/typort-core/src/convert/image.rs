use typort_ooxml::document::{ImageData, ImageFormat};
use typst::layout::{Frame, FrameItem, PagedDocument};

/// Extract all images from a `PagedDocument` by walking page frames.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn extract_images_from_paged(paged: &PagedDocument) -> Vec<ImageData> {
    let mut images = Vec::new();
    for page in &paged.pages {
        collect_frame_images(&page.frame, &mut images);
    }
    images
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn collect_frame_images(frame: &Frame, images: &mut Vec<ImageData>) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Image(img, size, _) => {
                // Always push one entry per image so the `<img>` FIFO stays aligned
                // with the HTML `<img>` tags. An unencodable image (e.g. an embedded
                // PDF) becomes an empty placeholder the consumer skips — never a
                // silent positional shift onto the wrong caption.
                images.push(convert_typst_image(img, size).unwrap_or_else(|| ImageData {
                    bytes: Vec::new(),
                    format: ImageFormat::Png,
                    width_emu: (size.x.to_pt() * 12700.0) as u64,
                    height_emu: (size.y.to_pt() * 12700.0) as u64,
                }));
            }
            // Skip drawing canvases: they are rasterized whole by
            // `extract_figure_rasters_from_paged`, so don't also pull any raster
            // nested inside them into the <img> queue.
            FrameItem::Group(group) if !frame_has_curve(&group.frame) => {
                collect_frame_images(&group.frame, images);
            }
            _ => {}
        }
    }
}

/// Rasterize every drawing canvas (a curve-bearing group — `CeTZ` plots,
/// diagrams) in the document to a PNG, in page order. Kept in a separate FIFO
/// from the raster/SVG `<img>` queue so the two never interleave: drawing
/// `<figure>`s pull from here, `<img>` tags pull from `extract_images_from_paged`.
pub(super) fn extract_figure_rasters_from_paged(paged: &PagedDocument) -> Vec<ImageData> {
    let mut out = Vec::new();
    for page in &paged.pages {
        collect_figure_rasters(&page.frame, &mut out);
    }
    out
}

fn collect_figure_rasters(frame: &Frame, out: &mut Vec<ImageData>) {
    for (_, item) in frame.items() {
        if let FrameItem::Group(group) = item {
            if frame_has_curve(&group.frame) {
                // Outermost drawing group: rasterize whole, don't descend.
                if let Some(data) = rasterize_frame(&group.frame) {
                    out.push(data);
                }
            } else {
                collect_figure_rasters(&group.frame, out);
            }
        }
    }
}

/// True if a frame (recursively) contains a Bézier curve shape — the signature
/// of vector line art. Tables and horizontal rules use only straight `Line`
/// geometry, so this never matches a table grid.
pub(super) fn frame_has_curve(frame: &Frame) -> bool {
    frame.items().any(|(_, item)| match item {
        FrameItem::Shape(shape, _) => {
            matches!(shape.geometry, typst::visualize::Geometry::Curve(_))
        }
        FrameItem::Group(group) => frame_has_curve(&group.frame),
        _ => false,
    })
}

/// Rasterize a single drawing frame to a PNG, sized at its on-page layout
/// dimensions (so Word reproduces the figure's footprint).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn rasterize_frame(frame: &Frame) -> Option<ImageData> {
    use typst::foundations::{Content, Smart};
    use typst::layout::Page;

    let size = frame.size();
    let (width_pt, height_pt) = (size.x.to_pt(), size.y.to_pt());
    if width_pt <= 0.0 || height_pt <= 0.0 {
        return None;
    }
    // 150 DPI: crisp line art without bloating the document.
    let pixel_per_pt = 150.0 / 72.0;
    let page = Page {
        frame: frame.clone(),
        fill: Smart::Auto, // -> white background for raster
        numbering: None,
        supplement: Content::empty(),
        number: 1,
    };
    let pixmap = typst_render::render(&page, pixel_per_pt);
    let bytes = pixmap.encode_png().ok()?;
    Some(ImageData {
        bytes,
        format: ImageFormat::Png,
        width_emu: (width_pt * 12700.0) as u64,
        height_emu: (height_pt * 12700.0) as u64,
    })
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn convert_typst_image(
    img: &typst::visualize::Image,
    size: &typst::layout::Size,
) -> Option<ImageData> {
    use typst_library::visualize::ImageKind;

    let width_emu = (size.x.to_pt() * 12700.0) as u64;
    let height_emu = (size.y.to_pt() * 12700.0) as u64;

    match img.kind() {
        ImageKind::Raster(raster) => {
            use typst_library::visualize::{ExchangeFormat, RasterFormat};
            // Word embeds PNG/JPEG directly; for GIF/WebP/Pixel (which Word can't
            // embed) re-encode the already-decoded image to PNG. Returning None would
            // drop the frame and desync the `<img>` FIFO onto the wrong captions.
            let (bytes, format) = match raster.format() {
                RasterFormat::Exchange(ExchangeFormat::Png) => {
                    (raster.data().to_vec(), ImageFormat::Png)
                }
                RasterFormat::Exchange(ExchangeFormat::Jpg) => {
                    (raster.data().to_vec(), ImageFormat::Jpeg)
                }
                RasterFormat::Exchange(ExchangeFormat::Gif | ExchangeFormat::Webp)
                | RasterFormat::Pixel(_) => {
                    let mut png = Vec::new();
                    raster
                        .dynamic()
                        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                        .ok()?;
                    (png, ImageFormat::Png)
                }
            };
            Some(ImageData {
                bytes,
                format,
                width_emu,
                height_emu,
            })
        }
        ImageKind::Svg(svg) => {
            let tree = svg.tree();
            let svg_size = tree.size();
            let pixel_w = svg_size.width().ceil() as u32;
            let pixel_h = svg_size.height().ceil() as u32;
            if pixel_w == 0 || pixel_h == 0 {
                return None;
            }
            let mut pixmap = tiny_skia::Pixmap::new(pixel_w, pixel_h)?;
            resvg::render(tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
            let png_bytes = pixmap.encode_png().ok()?;

            Some(ImageData {
                bytes: png_bytes,
                format: ImageFormat::Png,
                width_emu,
                height_emu,
            })
        }
        ImageKind::Pdf(_) => None,
    }
}
