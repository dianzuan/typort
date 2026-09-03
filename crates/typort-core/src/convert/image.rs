use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use typort_ooxml::document::{ImageData, ImageFormat};
use typst::introspection::Location;
use typst::layout::{Frame, FrameItem};
use typst_html::HtmlFrame;
use typst_layout::PagedDocument;

/// Image CONTENT comes from the HTML `<img>` src data-URL (the semantic
/// source, see `image_data_from_src`); the paged frames contribute only the
/// on-page DISPLAY SIZE. This maps a hash of each image's raw bytes to its
/// layouted size in EMU, so an `<img>` can look up how large Typst actually
/// drew it. Content and size are matched by bytes, not by position — there is
/// no ordering to desync.
pub(super) fn collect_image_sizes(paged: &PagedDocument) -> HashMap<u64, (u64, u64)> {
    let mut sizes = HashMap::new();
    for page in paged.pages() {
        collect_frame_image_sizes(&page.frame, &mut sizes);
    }
    sizes
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "positive pt sizes rounded to whole pixels"
)]
fn collect_frame_image_sizes(frame: &Frame, sizes: &mut HashMap<u64, (u64, u64)>) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Image(img, size, _) => {
                let bytes: Option<&[u8]> = match img.kind() {
                    typst_library::visualize::ImageKind::Raster(r) => Some(r.data().as_ref()),
                    typst_library::visualize::ImageKind::Svg(s) => Some(s.data().as_ref()),
                    typst_library::visualize::ImageKind::Pdf(_) => None,
                };
                if let Some(bytes) = bytes {
                    let emu = (
                        (size.x.to_pt() * 12700.0) as u64,
                        (size.y.to_pt() * 12700.0) as u64,
                    );
                    // First occurrence wins: document order matches the HTML.
                    sizes.entry(hash_bytes(bytes)).or_insert(emu);
                }
            }
            FrameItem::Group(group) => {
                collect_frame_image_sizes(&group.frame, sizes);
            }
            _ => {}
        }
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Build embeddable `ImageData` from an HTML `<img>` src attribute.
///
/// typst-html 0.15 embeds every image as `data:<mime>;base64,<data>` — the
/// image's own bytes travel WITH the `<img>` element, so content can never be
/// attached to the wrong tag (the failure mode of the old paged-order FIFO).
/// PNG/JPEG embed directly; GIF/WebP re-encode to PNG (Word can't embed
/// them); SVG (including PDFs, which typst-html converts to SVG) rasterizes
/// to PNG. `sizes` supplies the on-page display size by content hash, with
/// the intrinsic dimensions from the decoded data as fallback.
pub(super) fn image_data_from_src(
    src: &str,
    sizes: &HashMap<u64, (u64, u64)>,
) -> Option<ImageData> {
    use base64::Engine as _;

    let rest = src.strip_prefix("data:")?;
    let (mime, b64) = rest.split_once(";base64,")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let layout_size = sizes.get(&hash_bytes(&bytes)).copied();

    // Intrinsic pixel size → EMU (96 px/inch), used when the paged size is
    // unavailable (e.g. HTML-only conversion).
    let px_to_emu = |px: u32| u64::from(px) * 9525;

    match mime {
        "image/png" | "image/jpeg" => {
            let format = if mime == "image/png" {
                ImageFormat::Png
            } else {
                ImageFormat::Jpeg
            };
            let (w, h) = layout_size.or_else(|| {
                let dims = ::image::load_from_memory(&bytes).ok()?;
                Some((px_to_emu(dims.width()), px_to_emu(dims.height())))
            })?;
            Some(ImageData {
                bytes,
                format,
                width_emu: w,
                height_emu: h,
            })
        }
        "image/gif" | "image/webp" => {
            let decoded = ::image::load_from_memory(&bytes).ok()?;
            let (w, h) =
                layout_size.unwrap_or((px_to_emu(decoded.width()), px_to_emu(decoded.height())));
            let mut png = Vec::new();
            decoded
                .write_to(
                    &mut std::io::Cursor::new(&mut png),
                    ::image::ImageFormat::Png,
                )
                .ok()?;
            Some(ImageData {
                bytes: png,
                format: ImageFormat::Png,
                width_emu: w,
                height_emu: h,
            })
        }
        // Intentional f32→u32: SVG canvas dimensions are small positive values.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        "image/svg+xml" => {
            let mut options = resvg::usvg::Options::default();
            options.fontdb_mut().load_system_fonts();
            let tree = resvg::usvg::Tree::from_data(&bytes, &options).ok()?;
            let svg_size = tree.size();
            let pixel_w = svg_size.width().ceil() as u32;
            let pixel_h = svg_size.height().ceil() as u32;
            if pixel_w == 0 || pixel_h == 0 {
                return None;
            }
            let mut pixmap = tiny_skia::Pixmap::new(pixel_w, pixel_h)?;
            resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
            let png = pixmap.encode_png().ok()?;
            let (w, h) = layout_size.unwrap_or((px_to_emu(pixel_w), px_to_emu(pixel_h)));
            Some(ImageData {
                bytes: png,
                format: ImageFormat::Png,
                width_emu: w,
                height_emu: h,
            })
        }
        _ => None,
    }
}

/// Rasterize an `HtmlNode::Frame` — layouted content explicitly embedded into
/// the HTML export (`html.frame`), handed over as a laid-out frame in
/// document order. The docx embeds it as a PNG at its layout size.
pub(super) fn rasterize_html_frame(html_frame: &HtmlFrame) -> Option<ImageData> {
    rasterize_frame(&html_frame.inner)
}

/// Rasterize each figure's vector-drawing canvas, keyed by the OWNING
/// `FigureElem`'s introspection `Location`.
///
/// A drawing body (`#place`d curves, `CeTZ` canvases) is dropped entirely from
/// the HTML export — it exists only in the paged frames, whose tag brackets
/// (`Tag::Start`/`Tag::End` of each `FigureElem`) tell us exactly which
/// figure a canvas belongs to. Keyed consumption replaces the old page-order
/// FIFO, which attached rasters to whatever figure popped next: a quote or
/// code-listing figure stole the following canvas's image, and every later
/// drawing shifted one slot. Rasters of figures the HTML walk converts
/// through other paths (tables, images) simply stay unconsumed.
pub(super) fn extract_figure_rasters(paged: &PagedDocument) -> HashMap<Location, ImageData> {
    let mut rasters = HashMap::new();
    let mut stack: Vec<Location> = Vec::new();
    for page in paged.pages() {
        collect_figure_canvases(&page.frame, &mut stack, &mut rasters);
    }
    rasters
}

fn collect_figure_canvases(
    frame: &Frame,
    stack: &mut Vec<Location>,
    rasters: &mut HashMap<Location, ImageData>,
) {
    use typst::foundations::NativeElement;
    use typst::introspection::Tag;
    for (_, item) in frame.items() {
        match item {
            FrameItem::Tag(Tag::Start(content, _)) => {
                if content.elem() == typst_library::model::FigureElem::ELEM
                    && let Some(loc) = content.location()
                {
                    stack.push(loc);
                }
            }
            FrameItem::Tag(Tag::End(loc, ..)) => {
                if stack.last() == Some(loc) {
                    stack.pop();
                }
            }
            FrameItem::Group(group) => {
                if let Some(&owner) = stack.last()
                    && frame_has_curve(&group.frame)
                {
                    // Outermost curve-bearing group inside this figure: the
                    // canvas. First one wins; don't descend into it.
                    if !rasters.contains_key(&owner)
                        && let Some(data) = rasterize_frame(&group.frame)
                    {
                        rasters.insert(owner, data);
                    }
                } else {
                    collect_figure_canvases(&group.frame, stack, rasters);
                }
            }
            _ => {}
        }
    }
}

/// True if a frame (recursively) contains a Bézier curve shape — the signature
/// of vector line art. Tables and horizontal rules use only straight `Line`
/// geometry, so this never matches a table grid. Used by recovery to keep
/// canvas-internal text out of the recovered-line corpus.
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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "positive pt sizes rounded to whole pixels / EMU"
)]
fn rasterize_frame(frame: &Frame) -> Option<ImageData> {
    use typst::foundations::{Content, Smart};
    use typst::layout::{Abs, Sides};
    use typst::utils::Scalar;
    use typst_layout::Page;
    use typst_render::RenderOptions;

    let size = frame.size();
    let (width_pt, height_pt) = (size.x.to_pt(), size.y.to_pt());
    if width_pt <= 0.0 || height_pt <= 0.0 {
        return None;
    }
    // 150 DPI: crisp line art without bloating the document.
    let pixel_per_pt = 150.0 / 72.0;
    let page = Page {
        frame: frame.clone(),
        // No bleed: we rasterize the figure's own footprint exactly.
        bleed: Sides::splat(Abs::zero()),
        fill: Smart::Auto, // -> white background for raster
        numbering: None,
        supplement: Content::empty(),
        number: 1,
    };
    // `render` now takes `&RenderOptions` (an `f32` scale was removed in 0.15);
    // set the pixels-per-point scale on a defaulted options value.
    let opts = RenderOptions {
        pixel_per_pt: Scalar::new(pixel_per_pt),
        ..RenderOptions::default()
    };
    let pixmap = typst_render::render(&page, &opts);
    let bytes = pixmap.encode_png().ok()?;
    Some(ImageData {
        bytes,
        format: ImageFormat::Png,
        width_emu: (width_pt * 12700.0) as u64,
        height_emu: (height_pt * 12700.0) as u64,
    })
}
