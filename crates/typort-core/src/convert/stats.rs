//! Small statistical helpers shared by paged-style and recovery heuristics.

use std::cmp::Reverse;

/// Return the most frequent key, preferring the smallest key when counts tie.
pub(super) fn dominant_key<'a, K: Ord + ?Sized>(
    counts: impl IntoIterator<Item = (&'a K, usize)>,
) -> Option<&'a K> {
    counts
        .into_iter()
        .max_by_key(|(key, count)| (*count, Reverse(*key)))
        .map(|(key, _)| key)
}

/// Sort values and return the upper middle value used by the existing detectors.
pub(super) fn median(values: &mut [f64]) -> Option<f64> {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.get(values.len() / 2).copied()
}

/// Return the population standard deviation, or `None` for an empty sample.
pub(super) fn standard_deviation(values: &[f64]) -> Option<f64> {
    let count = u32::try_from(values.len()).ok().map(f64::from)?;
    let mean = values.iter().sum::<f64>() / count;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / count;
    Some(variance.sqrt())
}

/// Map an index proportionally with integer truncation.
pub(super) fn proportional_index(index: usize, source_len: usize, target_len: usize) -> usize {
    index.saturating_mul(target_len) / source_len.max(1)
}

/// Map an index proportionally, rounding to the nearest target index.
pub(super) fn proportional_index_rounded(
    index: usize,
    source_len: usize,
    target_len: usize,
) -> usize {
    let product = index.saturating_mul(target_len);
    product.saturating_add(source_len / 2) / source_len.max(1)
}
