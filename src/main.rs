//! imgdupe - find duplicate and near-duplicate images in a folder.
//!
//!     imgdupe ./photos
//!     imgdupe ./photos --threshold 8 --csv duplicates.csv
//!     imgdupe ./photos --delete-script cleanup.sh
//!
//! Reading and hashing runs on every core (rayon); the grouping is a plain
//! O(n^2) bit comparison, which is nothing next to the file I/O.
//!
//! Nothing is ever deleted. `--delete-script` writes the commands out for a
//! human to read and run.

mod group;
mod hash;

use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Parser;
use rayon::prelude::*;
use walkdir::WalkDir;

use group::{Entry, Group};

const EXTENSIONS: [&str; 9] = ["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff", "jfif"];

#[derive(Parser, Debug)]
#[command(name = "imgdupe", about = "Find duplicate and near-duplicate images")]
struct Args {
    /// Folder to scan
    folder: PathBuf,

    /// Hamming distance that still counts as the same picture (0 = identical only)
    #[arg(long, default_value_t = 5)]
    threshold: u32,

    /// Do not descend into sub-folders
    #[arg(long)]
    flat: bool,

    /// Ignore files smaller than this many kilobytes
    #[arg(long, default_value_t = 0)]
    min_kb: u64,

    /// Write the groups to a CSV
    #[arg(long)]
    csv: Option<PathBuf>,

    /// Write the delete commands to a script (never executed by this tool)
    #[arg(long)]
    delete_script: Option<PathBuf>,

    /// Print the hash of every file, not just the duplicates
    #[arg(long)]
    all: bool,
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn collect_files(args: &Args) -> Vec<PathBuf> {
    let walker = if args.flat {
        WalkDir::new(&args.folder).max_depth(1)
    } else {
        WalkDir::new(&args.folder)
    };
    walker
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| is_image(path))
        .collect()
}

/// Read + hash one file. Unreadable files are reported, not silently dropped.
fn hash_file(path: &Path, min_bytes: u64) -> Result<Option<Entry>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() < min_bytes {
        return Ok(None);
    }
    let image = image::open(path).map_err(|error| error.to_string())?;
    Ok(Some(Entry {
        path: path.display().to_string(),
        hash: hash::dhash(&image),
        bytes: metadata.len(),
        width: image.width(),
        height: image.height(),
    }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !args.folder.is_dir() {
        eprintln!("not a folder: {}", args.folder.display());
        std::process::exit(2);
    }

    let started = Instant::now();
    let files = collect_files(&args);
    if files.is_empty() {
        println!("no images found in {}", args.folder.display());
        return Ok(());
    }
    println!("{} image(s) found, hashing on {} threads...", files.len(), rayon::current_num_threads());

    let min_bytes = args.min_kb * 1024;
    let results: Vec<Result<Option<Entry>, (String, String)>> = files
        .par_iter()
        .map(|path| {
            hash_file(path, min_bytes).map_err(|error| (path.display().to_string(), error))
        })
        .collect();

    let mut entries = Vec::with_capacity(results.len());
    let mut failed = Vec::new();
    let mut skipped = 0usize;
    for result in results {
        match result {
            Ok(Some(entry)) => entries.push(entry),
            Ok(None) => skipped += 1,
            Err(problem) => failed.push(problem),
        }
    }

    let hashed_in = started.elapsed();
    let groups = group::group(&entries, args.threshold);
    let total_time = started.elapsed();

    report(&entries, &groups, &failed, skipped, hashed_in, total_time, args.threshold);

    if args.all {
        println!("Every file:");
        for entry in &entries {
            println!("  {}  {:>10}  {}", hash::to_hex(entry.hash), human(entry.bytes), entry.path);
        }
        println!();
    }
    if let Some(path) = &args.csv {
        write_csv(path, &groups)?;
    }
    if let Some(path) = &args.delete_script {
        write_delete_script(path, &groups)?;
    }

    std::process::exit(if groups.is_empty() { 0 } else { 1 });
}

#[allow(clippy::too_many_arguments)]
fn report(
    entries: &[Entry],
    groups: &[Group],
    failed: &[(String, String)],
    skipped: usize,
    hashed_in: std::time::Duration,
    total: std::time::Duration,
    threshold: u32,
) {
    let reclaimable: u64 = groups.iter().map(Group::reclaimable_bytes).sum();
    let duplicates: usize = groups.iter().map(|g| g.entries.len() - 1).sum();
    let rate = entries.len() as f64 / hashed_in.as_secs_f64().max(0.001);

    println!(
        "hashed {} image(s) in {:.1}s ({:.0}/s), grouped in {:.0}ms\n",
        entries.len(),
        hashed_in.as_secs_f64(),
        rate,
        (total - hashed_in).as_secs_f64() * 1000.0,
    );

    if groups.is_empty() {
        println!("No duplicates at threshold {threshold}. Nothing to clean up.\n");
    } else {
        println!(
            "{} group(s), {duplicates} redundant file(s), {} reclaimable\n",
            groups.len(),
            human(reclaimable)
        );
        for (index, group) in groups.iter().enumerate().take(20) {
            let kind = if group.is_exact() {
                "identical".to_string()
            } else {
                format!("near-identical (up to {} bits apart)", group.max_distance)
            };
            println!(
                "#{}  {}  -  {} files, {} reclaimable",
                index + 1,
                kind,
                group.entries.len(),
                human(group.reclaimable_bytes())
            );
            for (position, entry) in group.entries.iter().enumerate() {
                let marker = if position == 0 { "keep  " } else { "extra " };
                println!(
                    "    {marker} {:>5}x{:<5} {:>9}  {}",
                    entry.width,
                    entry.height,
                    human(entry.bytes),
                    entry.path
                );
            }
            println!();
        }
        if groups.len() > 20 {
            println!("... and {} more group(s), see --csv\n", groups.len() - 20);
        }
    }

    if skipped > 0 {
        println!("{skipped} file(s) skipped by --min-kb");
    }
    if !failed.is_empty() {
        println!("{} file(s) could not be read:", failed.len());
        for (path, error) in failed.iter().take(5) {
            println!("  {path}  -  {error}");
        }
        if failed.len() > 5 {
            println!("  ... and {} more", failed.len() - 5);
        }
    }
}

fn escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn write_csv(path: &Path, groups: &[Group]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    writeln!(file, "group,role,hash,width,height,bytes,max_distance,path")?;
    for (index, group) in groups.iter().enumerate() {
        for (position, entry) in group.entries.iter().enumerate() {
            writeln!(
                file,
                "{},{},{},{},{},{},{},{}",
                index + 1,
                if position == 0 { "keep" } else { "duplicate" },
                hash::to_hex(entry.hash),
                entry.width,
                entry.height,
                entry.bytes,
                group.max_distance,
                escape(&entry.path),
            )?;
        }
    }
    println!("{} group(s) -> {}", groups.len(), path.display());
    Ok(())
}

/// Write the deletions as a script instead of doing them.
///
/// Every one of these tools that deletes by itself eventually deletes the wrong
/// file. A script can be read, edited and version-controlled first.
fn write_delete_script(path: &Path, groups: &[Group]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    let reclaimable: u64 = groups.iter().map(Group::reclaimable_bytes).sum();

    writeln!(file, "#!/bin/sh")?;
    writeln!(file, "# Generated by imgdupe. NOTHING HAS BEEN DELETED YET.")?;
    writeln!(file, "# Read this file, delete the lines you disagree with, then run it.")?;
    writeln!(file, "# Frees {} across {} group(s).\n", human(reclaimable), groups.len())?;

    for (index, group) in groups.iter().enumerate() {
        writeln!(file, "# group {} - keeping {}", index + 1, group.keeper().path)?;
        for entry in group.entries.iter().skip(1) {
            writeln!(file, "rm -- {}", shell_quote(&entry.path))?;
        }
        writeln!(file)?;
    }
    println!("delete script -> {} (review before running; nothing was removed)", path.display());
    Ok(())
}

fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_extensions_are_recognised_case_insensitively() {
        assert!(is_image(Path::new("a/b/photo.JPG")));
        assert!(is_image(Path::new("thumb.webp")));
        assert!(!is_image(Path::new("notes.txt")));
        assert!(!is_image(Path::new("no_extension")));
    }

    #[test]
    fn sizes_are_printed_for_humans() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(2048), "2.0 KB");
        assert_eq!(human(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn paths_with_quotes_survive_shell_quoting() {
        assert_eq!(shell_quote("/tmp/it's here.jpg"), "'/tmp/it'\\''s here.jpg'");
        assert_eq!(shell_quote("/tmp/plain.jpg"), "'/tmp/plain.jpg'");
    }

    #[test]
    fn csv_fields_with_commas_are_quoted() {
        assert_eq!(escape("a,b"), "\"a,b\"");
        assert_eq!(escape("plain"), "plain");
        assert_eq!(escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
