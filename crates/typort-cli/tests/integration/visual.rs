//! Visual-regression tests: render docx via LibreOffice/pdftoppm/ImageMagick and RMSE-compare against Typst's own PDF.

use std::path::Path;

/// Compile a .typ to PDF via Typst's native renderer (ground truth).
fn typst_to_pdf(typ_path: &Path) -> Vec<u8> {
    let world = typort_core::TyportWorld::new(typ_path).unwrap();
    let paged = typst::compile::<typst_layout::PagedDocument>(&world)
        .output
        .unwrap();
    typst_pdf::pdf(&paged, &typst_pdf::PdfOptions::default()).unwrap()
}

/// Convert .typ → .docx → PDF (via LibreOffice), return PDF bytes.
fn typort_to_pdf_via_docx(typ_path: &Path, label: &str) -> Option<Vec<u8>> {
    let world = typort_core::TyportWorld::new(typ_path).unwrap();
    let doc = typort_core::convert::convert(&world).unwrap();
    let tmp_dir = std::env::temp_dir().join("typort_visual_test");
    std::fs::create_dir_all(&tmp_dir).ok()?;
    let docx_path = tmp_dir.join(format!("{label}.docx"));
    let f = std::fs::File::create(&docx_path).ok()?;
    typort_ooxml::write_docx(&doc, std::io::BufWriter::new(f)).ok()?;

    let status = std::process::Command::new("libreoffice")
        .args(["--headless", "--convert-to", "pdf", "--outdir"])
        .arg(&tmp_dir)
        .arg(&docx_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    std::fs::read(tmp_dir.join(format!("{label}.pdf"))).ok()
}

/// Render a PDF page to a PNG image using pdftoppm.
fn pdf_page_to_png(pdf_bytes: &[u8], page: u32, label: &str) -> Option<std::path::PathBuf> {
    let tmp_dir = std::env::temp_dir().join("typort_visual_test");
    std::fs::create_dir_all(&tmp_dir).ok()?;
    let pdf_path = tmp_dir.join(format!("{label}.pdf"));
    std::fs::write(&pdf_path, pdf_bytes).ok()?;
    let out_prefix = tmp_dir.join(format!("{label}_page"));
    let page_str = page.to_string();
    let status = std::process::Command::new("pdftoppm")
        .args(["-png", "-r", "150", "-f", &page_str, "-l", &page_str])
        .arg(&pdf_path)
        .arg(&out_prefix)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let png_name = format!("{label}_page-{page:0>2}.png",);
    let png_path = tmp_dir.join(&png_name);
    if png_path.exists() {
        Some(png_path)
    } else {
        // pdftoppm may use different padding
        let alt = tmp_dir.join(format!("{label}_page-{page}.png"));
        if alt.exists() { Some(alt) } else { None }
    }
}

/// Compare two PNG images using ImageMagick, return the normalized difference (0.0 = identical).
fn compare_images(a: &Path, b: &Path) -> Option<f64> {
    let output = std::process::Command::new("compare")
        .args(["-metric", "RMSE"])
        .arg(a)
        .arg(b)
        .arg("/dev/null")
        .output()
        .ok()?;
    // ImageMagick outputs metric to stderr: "1234.5 (0.0188)"
    let stderr = String::from_utf8_lossy(&output.stderr);
    let paren_start = stderr.find('(')?;
    let paren_end = stderr.find(')')?;
    stderr[paren_start + 1..paren_end].parse::<f64>().ok()
}

// The visual-regression tests render typort's docx to PDF (LibreOffice), to PNG
// (pdftoppm), and RMSE-compare against Typst's own PDF (ImageMagick). They are
// `#[ignore]`d because those tools are not in CI — but, when opted into with
// `cargo test -- --ignored`, a MISSING tool is a hard `panic!`, never a silent
// pass, so "ran but skipped" can no longer masquerade as "passed".
#[test]
#[ignore = "needs libreoffice + pdftoppm + ImageMagick; run with --ignored"]
fn visual_regression_hello() {
    let path = Path::new("../../tests/fixtures/hello.typ");
    let ground_truth = typst_to_pdf(path);
    let docx_pdf = typort_to_pdf_via_docx(path, "hello")
        .expect("libreoffice required: install it or do not opt into the --ignored visual tests");
    let gt_png =
        pdf_page_to_png(&ground_truth, 1, "gt_hello").expect("pdftoppm required for ground truth");
    let docx_png =
        pdf_page_to_png(&docx_pdf, 1, "docx_hello").expect("pdftoppm required for docx render");
    let diff = compare_images(&gt_png, &docx_png).expect("ImageMagick `compare` required");
    eprintln!("hello.typ visual diff: {diff:.4} (0=identical, <0.15=acceptable)");
    assert!(
        diff < 0.30,
        "visual regression too high for hello.typ: {diff:.4}"
    );
}

#[test]
#[ignore = "needs libreoffice + pdftoppm + ImageMagick; run with --ignored"]
fn visual_regression_complex_paper() {
    let path = Path::new("../../tests/fixtures/complex_paper.typ");
    let ground_truth = typst_to_pdf(path);
    let docx_pdf = typort_to_pdf_via_docx(path, "complex")
        .expect("libreoffice required: install it or do not opt into the --ignored visual tests");
    let gt_png = pdf_page_to_png(&ground_truth, 1, "gt_complex")
        .expect("pdftoppm required for ground truth");
    let docx_png =
        pdf_page_to_png(&docx_pdf, 1, "docx_complex").expect("pdftoppm required for docx render");
    let diff = compare_images(&gt_png, &docx_png).expect("ImageMagick `compare` required");
    eprintln!("complex_paper.typ visual diff: {diff:.4}");
    assert!(
        diff < 0.35,
        "visual regression too high for complex_paper.typ: {diff:.4}"
    );
}
