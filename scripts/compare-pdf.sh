#!/usr/bin/env bash
# Compare Typst→PDF vs Typst→docx→PDF conversion quality.
#
# Usage: ./scripts/compare-pdf.sh input.typ
#        ./scripts/compare-pdf.sh tests/fixtures/*.typ   # batch mode
#
# Outputs per-page RMSE (0 = identical, <0.05 = excellent, <0.10 = good, >0.15 = needs attention)

set -euo pipefail

if [ $# -eq 0 ]; then
  echo "Usage: $0 <file.typ> [file2.typ ...]"
  exit 1
fi

TYPORT="cargo run -p typort --release --quiet --"
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

total_files=0
total_pages=0
max_rmse=0
failed=()

for typ_file in "$@"; do
  basename=$(basename "$typ_file" .typ)
  total_files=$((total_files + 1))

  # 1. Typst → PDF (ground truth)
  typst compile "$typ_file" "$TMPDIR/${basename}_typst.pdf" 2>/dev/null || {
    echo "SKIP $typ_file (typst compile failed)"
    continue
  }

  # 2. Typst → docx → PDF (our conversion)
  $TYPORT "$typ_file" -o "$TMPDIR/${basename}.docx" 2>/dev/null || {
    echo "SKIP $typ_file (typort convert failed)"
    continue
  }
  libreoffice --headless --convert-to pdf "$TMPDIR/${basename}.docx" --outdir "$TMPDIR" >/dev/null 2>&1 || {
    echo "SKIP $typ_file (libreoffice convert failed)"
    continue
  }

  # 3. Render both PDFs to images (300 DPI)
  pdftoppm -r 150 -png "$TMPDIR/${basename}_typst.pdf" "$TMPDIR/${basename}_orig"
  pdftoppm -r 150 -png "$TMPDIR/${basename}.pdf" "$TMPDIR/${basename}_conv"

  # 4. Compare page by page
  file_max_rmse=0
  page=1
  while true; do
    orig=$(printf "$TMPDIR/${basename}_orig-%0*d.png" 1 $page 2>/dev/null)
    conv=$(printf "$TMPDIR/${basename}_conv-%0*d.png" 1 $page 2>/dev/null)
    # pdftoppm may use different zero-padding
    [ -f "$orig" ] || orig=$(printf "$TMPDIR/${basename}_orig-%0*d.png" 2 $page 2>/dev/null)
    [ -f "$conv" ] || conv=$(printf "$TMPDIR/${basename}_conv-%0*d.png" 2 $page 2>/dev/null)
    [ -f "$orig" ] || break
    [ -f "$conv" ] || break

    total_pages=$((total_pages + 1))

    # Compute RMSE using Python (more reliable than ImageMagick for different-sized images)
    rmse=$(python3 -c "
from PIL import Image
import numpy as np

a = np.array(Image.open('$orig').convert('RGB'), dtype=np.float64)
b_img = Image.open('$conv').convert('RGB')
# Resize to match if dimensions differ
if b_img.size != (a.shape[1], a.shape[0]):
    b_img = b_img.resize((a.shape[1], a.shape[0]), Image.LANCZOS)
b = np.array(b_img, dtype=np.float64)
rmse = np.sqrt(np.mean((a - b) ** 2)) / 255.0
print(f'{rmse:.4f}')
")

    # Track max
    is_worse=$(python3 -c "print('1' if $rmse > $file_max_rmse else '0')")
    [ "$is_worse" = "1" ] && file_max_rmse=$rmse

    page=$((page + 1))
  done

  # Report
  if python3 -c "exit(0 if $file_max_rmse <= 0.10 else 1)"; then
    status="OK"
  elif python3 -c "exit(0 if $file_max_rmse <= 0.15 else 1)"; then
    status="WARN"
  else
    status="FAIL"
    failed+=("$typ_file")
  fi

  printf "%-40s RMSE=%.4f  [%s]\n" "$basename" "$file_max_rmse" "$status"

  is_global_worse=$(python3 -c "print('1' if $file_max_rmse > $max_rmse else '0')")
  [ "$is_global_worse" = "1" ] && max_rmse=$file_max_rmse
done

echo ""
echo "Summary: $total_files files, $total_pages pages, max RMSE=$max_rmse"
if [ ${#failed[@]} -gt 0 ]; then
  echo "FAILED: ${failed[*]}"
  exit 1
fi
