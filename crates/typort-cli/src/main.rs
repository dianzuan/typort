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

    let doc = typort_core::convert_html(&world).unwrap_or_else(|errors| {
        eprintln!("error: Typst compilation failed:");
        for msg in &errors {
            eprintln!("  {msg}");
        }
        process::exit(1);
    });

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
