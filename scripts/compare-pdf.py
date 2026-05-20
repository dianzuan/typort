#!/usr/bin/env python3
"""Compare Typst→PDF vs Typst→docx→PDF by parsing PDF internal structure.

Uses PyMuPDF to extract text spans with position, font, size, and color,
then computes structural similarity metrics.

Usage:
    python3 scripts/compare-pdf.py original.pdf converted.pdf
    python3 scripts/compare-pdf.py tests/fixtures/hello.typ          # auto-pipeline
    python3 scripts/compare-pdf.py tests/fixtures/*.typ               # batch mode
"""

import os
import sys
import subprocess
import tempfile
from pathlib import Path

import fitz  # PyMuPDF

os.environ["PATH"] = os.path.expanduser("~/.cargo/bin") + ":" + os.environ.get("PATH", "")


def extract_text_spans(pdf_path):
    """Extract all text spans with position, font, size, color from a PDF."""
    doc = fitz.open(pdf_path)
    pages = []
    for page in doc:
        spans = []
        blocks = page.get_text("dict", flags=fitz.TEXT_PRESERVE_WHITESPACE)["blocks"]
        for block in blocks:
            if block["type"] != 0:  # text block
                continue
            for line in block["lines"]:
                for span in line["spans"]:
                    text = span["text"].strip()
                    if not text:
                        continue
                    spans.append({
                        "text": text,
                        "x": round(span["bbox"][0], 1),
                        "y": round(span["bbox"][1], 1),
                        "size": round(span["size"], 1),
                        "font": span["font"],
                        "color": span["color"],
                    })
        pages.append({
            "width": round(page.rect.width, 1),
            "height": round(page.rect.height, 1),
            "spans": spans,
        })
    doc.close()
    return pages


def compare_pages(orig_pages, conv_pages):
    """Compare two PDF page structures and return per-page metrics."""
    results = []
    max_pages = max(len(orig_pages), len(conv_pages))

    for i in range(max_pages):
        if i >= len(orig_pages):
            results.append({"page": i + 1, "status": "EXTRA_PAGE", "details": "converted has extra page"})
            continue
        if i >= len(conv_pages):
            results.append({"page": i + 1, "status": "MISSING_PAGE", "details": "converted missing page"})
            continue

        orig = orig_pages[i]
        conv = conv_pages[i]

        orig_texts = {s["text"] for s in orig["spans"]}
        conv_texts = {s["text"] for s in conv["spans"]}

        missing_text = orig_texts - conv_texts
        extra_text = conv_texts - orig_texts
        common_text = orig_texts & conv_texts

        # Position comparison for common text spans
        position_diffs = []
        size_diffs = []
        for orig_span in orig["spans"]:
            if orig_span["text"] not in common_text:
                continue
            # Find matching span in converted
            matches = [s for s in conv["spans"] if s["text"] == orig_span["text"]]
            if not matches:
                continue
            # Use closest y-position match
            best = min(matches, key=lambda s: abs(s["y"] - orig_span["y"]))
            dx = abs(best["x"] - orig_span["x"])
            dy = abs(best["y"] - orig_span["y"])
            position_diffs.append((dx, dy))
            ds = abs(best["size"] - orig_span["size"])
            if ds > 0.5:
                size_diffs.append((orig_span["text"][:20], orig_span["size"], best["size"]))

        # Compute metrics
        text_coverage = len(common_text) / max(len(orig_texts), 1)
        avg_dx = sum(d[0] for d in position_diffs) / max(len(position_diffs), 1)
        avg_dy = sum(d[1] for d in position_diffs) / max(len(position_diffs), 1)
        max_dy = max((d[1] for d in position_diffs), default=0)

        page_result = {
            "page": i + 1,
            "text_coverage": round(text_coverage, 3),
            "avg_x_offset": round(avg_dx, 1),
            "avg_y_offset": round(avg_dy, 1),
            "max_y_offset": round(max_dy, 1),
            "missing_texts": sorted(missing_text)[:5],
            "size_mismatches": size_diffs[:5],
        }

        # Status
        if text_coverage >= 0.95 and avg_dy < 10 and not size_diffs:
            page_result["status"] = "GOOD"
        elif text_coverage >= 0.85 and avg_dy < 30:
            page_result["status"] = "WARN"
        else:
            page_result["status"] = "FAIL"

        results.append(page_result)

    return results


def run_pipeline(typ_file, tmpdir):
    """Run full Typst→PDF and Typst→docx→PDF pipeline, return both PDF paths."""
    typ_path = Path(typ_file).resolve()
    tmp = Path(tmpdir)
    basename = typ_path.stem
    orig_pdf = tmp / f"{basename}_orig.pdf"
    docx = tmp / f"{basename}.docx"
    conv_pdf = tmp / f"{basename}.pdf"

    # Typst → PDF
    r = subprocess.run(
        ["typst", "compile", str(typ_path), str(orig_pdf)],
        capture_output=True, text=True
    )
    if r.returncode != 0:
        return None, None, f"typst compile failed: {r.stderr[:200]}"

    # Typst → docx
    r = subprocess.run(
        ["cargo", "run", "-p", "typort", "--release", "--quiet", "--",
         str(typ_path), "-o", str(docx)],
        capture_output=True, text=True
    )
    if r.returncode != 0:
        return None, None, f"typort failed: {r.stderr[:200]}"

    # docx → PDF
    r = subprocess.run(
        ["libreoffice", "--headless", "--convert-to", "pdf",
         str(docx), "--outdir", str(tmp)],
        capture_output=True, text=True
    )
    if r.returncode != 0:
        return None, None, f"libreoffice failed: {r.stderr[:200]}"

    return str(orig_pdf), str(conv_pdf), None


def main():
    if len(sys.argv) < 2:
        print("Usage: compare-pdf.py <file.typ|original.pdf> [converted.pdf]")
        sys.exit(1)

    files = sys.argv[1:]
    all_good = True

    with tempfile.TemporaryDirectory() as tmpdir:
      for f in files:
        path = Path(f)

        if path.suffix == ".typ":
            # Auto-pipeline mode
            print(f"\n{'='*60}")
            print(f"  {path.name}")
            print(f"{'='*60}")
            orig_pdf, conv_pdf, err = run_pipeline(f, tmpdir)
            if err:
                print(f"  SKIP: {err}")
                continue
        elif path.suffix == ".pdf" and len(files) >= 2:
            orig_pdf = files[0]
            conv_pdf = files[1]
            files = []  # consume both
        else:
            print(f"  SKIP: unsupported file {f}")
            continue

        orig_pages = extract_text_spans(orig_pdf)
        conv_pages = extract_text_spans(conv_pdf)

        results = compare_pages(orig_pages, conv_pages)

        for r in results:
            status = r["status"]
            icon = {"GOOD": "✓", "WARN": "△", "FAIL": "✗"}.get(status, "?")
            page = r["page"]

            if status in ("EXTRA_PAGE", "MISSING_PAGE"):
                print(f"  p{page} {icon} {r['details']}")
                all_good = False
                continue

            cov = r["text_coverage"]
            dy = r["avg_y_offset"]
            print(f"  p{page} {icon} text={cov:.0%}  y-offset={dy:.1f}pt  max-y={r['max_y_offset']:.1f}pt", end="")

            if r["missing_texts"]:
                print(f"  missing: {r['missing_texts']}", end="")
            if r["size_mismatches"]:
                for txt, orig_s, conv_s in r["size_mismatches"]:
                    print(f"  size[{txt}]: {orig_s}→{conv_s}", end="")
            print()

            if status != "GOOD":
                all_good = False

      if not all_good:
          sys.exit(1)


if __name__ == "__main__":
    main()
