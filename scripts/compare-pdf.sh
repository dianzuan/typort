#!/bin/sh
# Compare Typst→PDF with Typst→docx→PDF conversion quality.
#
# Usage: ./scripts/compare-pdf.sh input.typ [input2.typ ...]
#        FIXTURES='tests/fixtures/*.typ' ./scripts/compare-pdf.sh
#
# Outputs per-file maximum normalized RMSE. A score of 0 is identical,
# at most 0.10 is good, 0.10–0.15 is a warning, and above 0.15 fails.

set -eu

if [ "$#" -eq 0 ]; then
    if [ -z "${FIXTURES:-}" ]; then
        echo "Usage: $0 <file.typ> [file2.typ ...]" >&2
        echo "   or: FIXTURES='tests/fixtures/*.typ' $0" >&2
        exit 1
    fi
    # Word splitting and glob expansion are intentional: FIXTURES is a
    # shell-style selector such as tests/fixtures/*.typ.
    # shellcheck disable=SC2086
    set -- $FIXTURES
fi

for tool in cargo typst libreoffice pdftoppm identify convert compare awk; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "error: required tool '$tool' is not installed" >&2
        exit 1
    fi
done

tmpdir=$(mktemp -d)
cleanup() {
    if [ -n "$tmpdir" ] && [ -d "$tmpdir" ]; then
        rm -rf -- "$tmpdir"
    fi
}
trap cleanup EXIT HUP INT TERM

total_files=0
total_pages=0
max_rmse=0
failed=

for typ_file in "$@"; do
    total_files=$((total_files + 1))
    basename=$(basename "$typ_file" .typ)
    file_dir="$tmpdir/file-$total_files"
    mkdir "$file_dir"

    if ! typst compile "$typ_file" "$file_dir/original.pdf" 2>/dev/null; then
        echo "SKIP $typ_file (typst compile failed)"
        failed="${failed}${failed:+ }$typ_file"
        continue
    fi

    if ! cargo run -p typort --release --quiet -- \
        "$typ_file" -o "$file_dir/converted.docx" 2>/dev/null; then
        echo "SKIP $typ_file (typort convert failed)"
        failed="${failed}${failed:+ }$typ_file"
        continue
    fi
    if ! libreoffice --headless --convert-to pdf "$file_dir/converted.docx" \
        --outdir "$file_dir" >/dev/null 2>&1; then
        echo "SKIP $typ_file (libreoffice convert failed)"
        failed="${failed}${failed:+ }$typ_file"
        continue
    fi

    pdftoppm -r 150 -png "$file_dir/original.pdf" "$file_dir/original"
    pdftoppm -r 150 -png "$file_dir/converted.pdf" "$file_dir/converted"
    find "$file_dir" -name 'original-*.png' -print | sort >"$file_dir/original-pages"
    find "$file_dir" -name 'converted-*.png' -print | sort >"$file_dir/converted-pages"

    file_max_rmse=0
    page=1
    exec 3<"$file_dir/original-pages"
    exec 4<"$file_dir/converted-pages"
    while :; do
        orig=
        conv=
        if IFS= read -r orig <&3; then
            has_orig=true
        else
            has_orig=false
        fi
        if IFS= read -r conv <&4; then
            has_conv=true
        else
            has_conv=false
        fi

        if [ "$has_orig" = false ] && [ "$has_conv" = false ]; then
            break
        fi
        total_pages=$((total_pages + 1))

        if [ "$has_orig" = false ]; then
            echo "  page $page: converted PDF has an extra page"
            rmse=1.0000
        elif [ "$has_conv" = false ]; then
            echo "  page $page: converted PDF is missing this page"
            rmse=1.0000
        else
            geometry=$(identify -format '%wx%h' "$orig")
            resized="$file_dir/converted-resized-$page.png"
            convert "$conv" -filter Lanczos -resize "${geometry}!" "$resized"

            compare_status=0
            metric=$(compare -metric RMSE "$orig" "$resized" null: 2>&1) \
                || compare_status=$?
            if [ "$compare_status" -gt 1 ]; then
                echo "error: ImageMagick comparison failed for $typ_file page $page" >&2
                exit "$compare_status"
            fi
            normalized=$(printf '%s\n' "$metric" | awk -F '[()]' 'NF >= 2 { print $2; exit }')
            if [ -z "$normalized" ]; then
                echo "error: could not parse ImageMagick RMSE for $typ_file page $page" >&2
                exit 1
            fi
            rmse=$(awk -v value="$normalized" 'BEGIN { printf "%.4f", value }')
        fi

        if awk -v current="$rmse" -v maximum="$file_max_rmse" \
            'BEGIN { exit !(current > maximum) }'; then
            file_max_rmse=$rmse
        fi
        page=$((page + 1))
    done
    exec 3<&-
    exec 4<&-

    if awk -v value="$file_max_rmse" 'BEGIN { exit !(value <= 0.10) }'; then
        status=OK
    elif awk -v value="$file_max_rmse" 'BEGIN { exit !(value <= 0.15) }'; then
        status=WARN
    else
        status=FAIL
        failed="${failed}${failed:+ }$typ_file"
    fi

    printf '%-40s RMSE=%.4f  [%s]\n' "$basename" "$file_max_rmse" "$status"
    if awk -v current="$file_max_rmse" -v maximum="$max_rmse" \
        'BEGIN { exit !(current > maximum) }'; then
        max_rmse=$file_max_rmse
    fi
done

printf '\nSummary: %d files, %d pages, max RMSE=%s\n' "$total_files" "$total_pages" "$max_rmse"
if [ -n "$failed" ]; then
    echo "FAILED: $failed"
    exit 1
fi
