// Typst's HTML export shatters this line into many text/space nodes around the
// bold span, and the per-run style pass paints each one separately. The
// run-coalescing post-pass must collapse the plain runs back together while
// keeping the bold word as its own styled run.

This is a fairly long plain line with one #strong[bold] word inside it here.
