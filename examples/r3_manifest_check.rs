use craftsman_fortress::asset_manifest::AssetManifest;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("R3 manifest check failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let manifest_path = args
        .next()
        .map(PathBuf::from)
        .ok_or("usage: r3_manifest_check <manifest.tsv> [--check-files <assets-root>]")?;
    let mut asset_root = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check-files" => {
                asset_root = Some(PathBuf::from(
                    args.next().ok_or("--check-files requires an assets root")?,
                ));
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    let manifest = AssetManifest::load(&manifest_path)?;
    manifest.validate_acceptance_ready()?;
    if let Some(root) = asset_root.as_deref() {
        manifest.validate_files(root)?;
    }
    println!("R3 manifest PASS");
    println!("entries={}", manifest.entries.len());
    println!("canonical_hash={:#018x}", manifest.canonical_hash());
    println!("archive_sha256={}", manifest.archive_sha256);
    Ok(())
}
