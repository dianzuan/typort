//! Shared depth-first traversal of paged frames.

use typst::layout::{Frame, FrameItem, Point};

/// Visit frame items in paint order, reporting each item's absolute position.
///
/// When `skip_curve_groups` is true, groups containing curves are omitted with
/// their descendants. Recovery uses this for drawing canvases whose text and
/// lines are already represented by a rasterized image.
pub(super) fn visit_frame_items<'a>(
    frame: &'a Frame,
    skip_curve_groups: bool,
    visitor: &mut impl FnMut(Point, &'a FrameItem),
) {
    visit_frame_items_at(frame, Point::zero(), skip_curve_groups, visitor);
}

fn visit_frame_items_at<'a>(
    frame: &'a Frame,
    offset: Point,
    skip_curve_groups: bool,
    visitor: &mut impl FnMut(Point, &'a FrameItem),
) {
    for (position, item) in frame.items() {
        let absolute = Point::new(offset.x + position.x, offset.y + position.y);
        if let FrameItem::Group(group) = item
            && skip_curve_groups
            && super::image::frame_has_curve(&group.frame)
        {
            continue;
        }
        visitor(absolute, item);
        if let FrameItem::Group(group) = item {
            visit_frame_items_at(&group.frame, absolute, skip_curve_groups, visitor);
        }
    }
}
