use std::fs::File;
use std::path::PathBuf;
use std::process;

use clap::Parser;

#[derive(Parser)]
#[command(name = "typort", about = "Convert Typst documents to Word (.docx)")]
struct Cli {
    /// Input .typ file
    input: PathBuf,

    /// Output .docx file
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Journal preset name (loads from presets/ directory)
    #[arg(long)]
    preset: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let output_path = cli
        .output
        .unwrap_or_else(|| cli.input.with_extension("docx"));

    let world = typort_core::TyportWorld::new(&cli.input).unwrap_or_else(|e| {
        eprintln!("error: failed to read input: {e}");
        process::exit(1);
    });

    let mut doc = typort_core::convert_html(&world).unwrap_or_else(|errors| {
        eprintln!("error: Typst compilation failed:");
        for msg in &errors {
            eprintln!("  {msg}");
        }
        process::exit(1);
    });

    // Apply preset if specified
    if let Some(preset_name) = &cli.preset {
        let preset = typort_presets::load_builtin_preset(preset_name).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            process::exit(1);
        });
        apply_preset(&mut doc, &preset);
    }

    let file = File::create(&output_path).unwrap_or_else(|e| {
        eprintln!("error: cannot create output file: {e}");
        process::exit(1);
    });

    typort_ooxml::write_docx(&doc, file).unwrap_or_else(|e| {
        eprintln!("error: failed to write .docx: {e}");
        process::exit(1);
    });

    println!("wrote {}", output_path.display());
}

fn apply_preset(doc: &mut typort_ooxml::Document, preset: &typort_presets::Preset) {
    if let Some(page) = &preset.page {
        if let Some(top) = page.margin_top_cm {
            doc.page_settings.margin_top = typort_presets::cm_to_twips(top);
        }
        if let Some(bottom) = page.margin_bottom_cm {
            doc.page_settings.margin_bottom = typort_presets::cm_to_twips(bottom);
        }
        if let Some(left) = page.margin_left_cm {
            doc.page_settings.margin_left = typort_presets::cm_to_twips(left);
        }
        if let Some(right) = page.margin_right_cm {
            doc.page_settings.margin_right = typort_presets::cm_to_twips(right);
        }
    }
    if let Some(footnote) = &preset.footnote
        && let Some(fmt) = &footnote.format
    {
        doc.style.footnote_format = match fmt.as_str() {
            "circled" => typort_ooxml::FootnoteFormat::CircledNumber,
            _ => typort_ooxml::FootnoteFormat::Decimal,
        };
    }
}
