//! Per-cell column-width fidelity for tables.
//!
//! Word stores table-cell widths as fiftieths of a percent of the table width
//! (`w:tcW w:type="pct"`, 5000 = 100%). Typst declares column tracks
//! semantically on the `TableElem`/`GridElem` `columns` field (a `TrackSizings`,
//! i.e. a list of [`Sizing`]). This module turns that declared track list into
//! per-column percentage shares so a `columns: (1fr, 2fr, 3fr)` spec becomes a
//! 16.7% : 33.3% : 50% split instead of three equal columns.
//!
//! Track kinds and how they map:
//! * `Sizing::Fr(fr)`  — a fraction of the remaining free space. Splits the
//!   width that fixed/relative tracks leave over, proportional to its `fr`.
//! * `Sizing::Rel(rel)` — a ratio of the table width plus an absolute
//!   (`pt`/`cm`/`mm`/`em`) length; resolved to points against the page content
//!   width, then expressed as a percentage.
//! * `Sizing::Auto` — fits its content, which we cannot measure here, so autos
//!   share the leftover space equally (Word's own default behaviour).
//!
//! A spec that is *only* autos — which includes the integer form `columns: 3`
//! (it casts to `Auto, Auto, Auto`) — carries no width signal, so we return
//! `None` and let the writer keep its equal-distribution fallback.

use typst_library::layout::{Length, Rel, Sizing};

/// Total cell width budget in fiftieths of a percent (5000 = 100%).
const PCT_TOTAL: f64 = 5000.0;
const PCT_TOTAL_U32: u32 = 5000;

/// Inputs needed to resolve absolute (`Rel<Length>`) tracks to a percentage of
/// the table width. Bundled so callers thread one value, not two positionals.
#[derive(Clone, Copy)]
pub(super) struct TableWidthCtx {
    /// Page content width (text area) in points; the table fills this at 100%.
    pub content_pt: f64,
    /// Body font size in points, used to resolve `em`-based track lengths.
    pub body_font_pt: f64,
}

/// Compute per-column width shares (in fiftieths of a percent of the 5000 table
/// width) for the given track sizes.
///
/// Returns `None` when the spec carries no usable width signal (every track is
/// `Auto`, e.g. `columns: 3`), so the caller leaves widths unset and the writer
/// distributes them equally.
pub(super) fn track_widths_pct(tracks: &[Sizing], ctx: TableWidthCtx) -> Option<Vec<u32>> {
    if tracks.is_empty() || tracks.iter().all(|s| matches!(s, Sizing::Auto)) {
        return None;
    }

    // 1. Resolve every fixed/relative track to an absolute percentage share.
    //    `Fr` and `Auto` tracks are placeholders for now (0.0).
    let mut shares = vec![0.0_f64; tracks.len()];
    let mut fixed_pct = 0.0;
    let mut fr_total = 0.0;
    let mut auto_count = 0u32;

    for (i, track) in tracks.iter().enumerate() {
        match track {
            Sizing::Rel(rel) => {
                let pct = rel_track_pct(rel, ctx);
                shares[i] = pct;
                fixed_pct += pct;
            }
            Sizing::Fr(fr) => fr_total += fr.get().max(0.0),
            Sizing::Auto => auto_count += 1,
        }
    }

    // 2. Distribute the leftover. Fractional tracks split it in proportion to
    //    their `fr`; if there are no `fr` tracks, autos share it equally.
    let leftover = (PCT_TOTAL - fixed_pct).max(0.0);
    if fr_total > 0.0 {
        for (i, track) in tracks.iter().enumerate() {
            if let Sizing::Fr(fr) = track {
                shares[i] = leftover * (fr.get().max(0.0) / fr_total);
            }
        }
        // Autos alongside fr tracks get nothing extra (fr eats the free space,
        // matching Typst).
    } else if auto_count > 0 {
        let each = leftover / f64::from(auto_count);
        for (i, track) in tracks.iter().enumerate() {
            if matches!(track, Sizing::Auto) {
                shares[i] = each;
            }
        }
    }

    Some(normalize_to_total(&shares))
}

/// Resolve one `Rel<Length>` track to a percentage of the table width.
fn rel_track_pct(rel: &Rel<Length>, ctx: TableWidthCtx) -> f64 {
    // Ratio part is already a fraction of the table width.
    let ratio_share = rel.rel.get() * PCT_TOTAL;
    // Absolute part: pt/cm/mm/em -> points -> fraction of content width.
    let points = rel.abs.abs.to_pt() + rel.abs.em.get() * ctx.body_font_pt;
    let length_share = if ctx.content_pt > 0.0 {
        (points / ctx.content_pt) * PCT_TOTAL
    } else {
        0.0
    };
    (ratio_share + length_share).max(0.0)
}

/// Round shares to integers, summing to at most `PCT_TOTAL`. The remainder from
/// rounding is folded into the widest column so the row still totals ~100%.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "shares are clamped to [0, 5000] before the f64 -> u32 cast"
)]
fn normalize_to_total(shares: &[f64]) -> Vec<u32> {
    let sum: f64 = shares.iter().sum();
    // Scale into the 5000 budget when fixed tracks overflowed it.
    let scale = if sum > PCT_TOTAL {
        PCT_TOTAL / sum
    } else {
        1.0
    };
    let mut out: Vec<u32> = shares
        .iter()
        .map(|s| (s * scale).round().clamp(0.0, PCT_TOTAL) as u32)
        .collect();
    // Fold any rounding remainder into the largest column.
    let total: u32 = out.iter().sum();
    if total < PCT_TOTAL_U32
        && let Some(idx) = out
            .iter()
            .enumerate()
            .max_by_key(|(_, w)| **w)
            .map(|(i, _)| i)
    {
        out[idx] += PCT_TOTAL_U32 - total;
    }
    out
}

/// Apply per-column shares onto a built table's cells. A cell spanning `colspan`
/// logical columns gets the sum of the spanned tracks' shares.
pub(super) fn assign_cell_widths(table: &mut typort_ooxml::document::Table, col_pct: &[u32]) {
    for row in &mut table.rows {
        let mut col = 0usize;
        for cell in &mut row.cells {
            let span = usize::try_from(cell.colspan.max(1)).unwrap_or(1);
            let width: u32 = col_pct.iter().skip(col).take(span).copied().sum();
            if width > 0 {
                cell.width_pct = Some(width);
            }
            col += span;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst_library::layout::{Abs, Em, Fr, Length, Rel, Sizing};

    fn ctx() -> TableWidthCtx {
        // A4 text area ~ 451pt; 11pt body. Only matters for Rel tracks.
        TableWidthCtx {
            content_pt: 451.0,
            body_font_pt: 11.0,
        }
    }

    #[test]
    fn fr_tracks_split_proportionally() {
        let tracks = [
            Sizing::Fr(Fr::new(1.0)),
            Sizing::Fr(Fr::new(2.0)),
            Sizing::Fr(Fr::new(3.0)),
        ];
        let w = track_widths_pct(&tracks, ctx()).expect("fr tracks carry a signal");
        assert_eq!(w.len(), 3);
        // 1:2:3 of 5000 -> ~833 / ~1667 / 2500, summing to 5000.
        assert!((800..=870).contains(&w[0]), "col0 = {}", w[0]);
        assert!((1630..=1700).contains(&w[1]), "col1 = {}", w[1]);
        assert!((2450..=2550).contains(&w[2]), "col2 = {}", w[2]);
        assert!(w[0] < w[1] && w[1] < w[2], "must strictly increase: {w:?}");
        assert_eq!(w.iter().sum::<u32>(), 5000, "row must total 100%");
    }

    #[test]
    fn all_auto_returns_none() {
        // `columns: 3` casts to Auto, Auto, Auto — no width signal.
        let tracks = [Sizing::Auto, Sizing::Auto, Sizing::Auto];
        assert!(track_widths_pct(&tracks, ctx()).is_none());
        assert!(track_widths_pct(&[], ctx()).is_none());
    }

    #[test]
    fn fixed_length_track_becomes_percentage() {
        // 2cm of a 451pt content width -> ~12.6% -> ~628 fiftieths.
        let two_cm = Rel::<Length>::from(Length {
            abs: Abs::cm(2.0),
            em: Em::zero(),
        });
        let tracks = [Sizing::Rel(two_cm), Sizing::Fr(Fr::new(1.0))];
        let w = track_widths_pct(&tracks, ctx()).expect("rel+fr carries a signal");
        assert!((560..=700).contains(&w[0]), "fixed col = {}", w[0]);
        assert!(w[1] > w[0], "fr column eats the rest: {w:?}");
        assert_eq!(w.iter().sum::<u32>(), 5000);
    }
}
