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
                if let Some(data) = convert_typst_image(img, size) {
                    images.push(data);
                }
            }
            FrameItem::Group(group) => {
                collect_frame_images(&group.frame, images);
            }
            _ => {}
        }
    }
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
            use typst_library::visualize::ExchangeFormat;
            let bytes = raster.data().to_vec();
            let format = match raster.format() {
                typst_library::visualize::RasterFormat::Exchange(ExchangeFormat::Png) => {
                    ImageFormat::Png
                }
                typst_library::visualize::RasterFormat::Exchange(ExchangeFormat::Jpg) => {
                    ImageFormat::Jpeg
                }
                typst_library::visualize::RasterFormat::Exchange(
                    ExchangeFormat::Gif | ExchangeFormat::Webp,
                )
                | typst_library::visualize::RasterFormat::Pixel(_) => return None,
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
