use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use clap::{ArgAction, Parser};
use walkdir::WalkDir;

const OTHERS_DIR: &str = "\u{305d}\u{306e}\u{4ed6}";

#[derive(Parser, Debug)]
#[command(author, version, about = "Sort photos by EXIF date (YYYYMMDD) into another directory")]
struct Args {
    /// Source directory to scan recursively.
    #[arg(short, long)]
    source: PathBuf,

    /// Destination root directory.
    #[arg(short, long)]
    destination: PathBuf,

    /// Dry-run report output file path.
    #[arg(long, default_value = "selected_picture_result.txt")]
    report_file: PathBuf,

    /// Execute actual move (dry-run is default).
    #[arg(long, action = ArgAction::SetTrue)]
    execute: bool,
}

fn main() {
    let args = Args::parse();

    if let Err(err) = run(args) {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    let source = canonicalize_existing_dir(&args.source, "source")?;
    let destination = ensure_destination_path(&args.destination)?;

    validate_paths(&source, &destination)?;

    let dry_run = !args.execute;
    let plan = build_plan(&source);

    if dry_run {
        write_dry_run_report(&plan, &destination, &args.report_file)?;
        println!(
            "Dry-run done: {} files analyzed. Report: {}",
            plan.values().map(std::vec::Vec::len).sum::<usize>(),
            args.report_file.display()
        );
        return Ok(());
    }

    fs::create_dir_all(&destination)
        .map_err(|e| format!("Failed to create destination {}: {e}", destination.display()))?;

    let mut moved_count = 0usize;
    for (bucket, files) in plan {
        let bucket_dir = destination.join(bucket);
        fs::create_dir_all(&bucket_dir)
            .map_err(|e| format!("Failed to create directory {}: {e}", bucket_dir.display()))?;

        for src_path in files {
            let file_name = src_path
                .file_name()
                .ok_or_else(|| format!("Unable to resolve file name: {}", src_path.display()))?;
            let dest_path = unique_destination_path(&bucket_dir, file_name);
            move_file(&src_path, &dest_path)?;
            moved_count += 1;
        }
    }

    println!("Done: moved {} files.", moved_count);
    Ok(())
}

fn canonicalize_existing_dir(path: &Path, label: &str) -> Result<PathBuf, String> {
    let metadata =
        fs::metadata(path).map_err(|e| format!("{label} does not exist: {} ({e})", path.display()))?;
    if !metadata.is_dir() {
        return Err(format!("{label} must be a directory: {}", path.display()));
    }
    fs::canonicalize(path).map_err(|e| format!("Failed to canonicalize {label}: {} ({e})", path.display()))
}

fn ensure_destination_path(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        let metadata = fs::metadata(path)
            .map_err(|e| format!("Cannot access destination {} ({e})", path.display()))?;
        if !metadata.is_dir() {
            return Err(format!("destination must be a directory: {}", path.display()));
        }
        return fs::canonicalize(path)
            .map_err(|e| format!("Failed to canonicalize destination {} ({e})", path.display()));
    }

    let base = if let Some(parent) = path.parent() {
        if parent.as_os_str().is_empty() {
            std::env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?
        } else if parent.exists() {
            fs::canonicalize(parent)
                .map_err(|e| format!("Failed to canonicalize destination parent {} ({e})", parent.display()))?
        } else {
            return Err(format!("Destination parent does not exist: {}", parent.display()));
        }
    } else {
        std::env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?
    };

    let name = path
        .file_name()
        .ok_or_else(|| format!("Invalid destination path: {}", path.display()))?;

    Ok(base.join(name))
}

fn validate_paths(source: &Path, destination: &Path) -> Result<(), String> {
    if source == destination {
        return Err("source and destination must be different directories".to_string());
    }

    if destination.starts_with(source) {
        return Err(format!(
            "destination cannot be under source: source={} destination={}",
            source.display(),
            destination.display()
        ));
    }

    Ok(())
}

fn build_plan(source: &Path) -> BTreeMap<String, Vec<PathBuf>> {
    let mut buckets: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();

    for entry in WalkDir::new(source)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let file_path = entry.path().to_path_buf();

        let bucket = if is_image_file(&file_path) {
            extract_exif_date(&file_path).unwrap_or_else(|| OTHERS_DIR.to_string())
        } else {
            OTHERS_DIR.to_string()
        };

        buckets.entry(bucket).or_default().push(file_path);
    }

    for files in buckets.values_mut() {
        files.sort();
    }

    buckets
}

fn is_image_file(path: &Path) -> bool {
    const IMAGE_EXTENSIONS: &[&str] = &[
        "jpg", "jpeg", "png", "heic", "heif", "tif", "tiff", "bmp", "gif", "webp",
    ];

    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn extract_exif_date(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif_reader = exif::Reader::new();
    let exif = exif_reader.read_from_container(&mut reader).ok()?;

    let field = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTimeDigitized, exif::In::PRIMARY))
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))?;

    parse_exif_datetime(&field.display_value().to_string())
}

fn parse_exif_datetime(value: &str) -> Option<String> {
    NaiveDateTime::parse_from_str(value, "%Y:%m:%d %H:%M:%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok())
        .map(|dt| dt.format("%Y%m%d").to_string())
}

fn write_dry_run_report(
    plan: &BTreeMap<String, Vec<PathBuf>>,
    destination: &Path,
    report_file: &Path,
) -> Result<(), String> {
    let mut output = String::new();
    output.push_str(&format!("{}\n", destination.display()));

    let total_groups = plan.len();
    for (group_idx, (bucket, files)) in plan.iter().enumerate() {
        let is_last_group = group_idx + 1 == total_groups;
        let group_prefix = if is_last_group { "└──" } else { "├──" };
        output.push_str(&format!("{} {}/\n", group_prefix, bucket));

        let total_files = files.len();
        for (file_idx, file_path) in files.iter().enumerate() {
            let is_last_file = file_idx + 1 == total_files;
            let branch = if is_last_group { "    " } else { "│   " };
            let file_prefix = if is_last_file { "└──" } else { "├──" };
            let name = file_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("<invalid-filename>");
            output.push_str(&format!("{}{} {}\n", branch, file_prefix, name));
        }
    }

    let mut file = File::create(report_file)
        .map_err(|e| format!("Failed to create report file {} ({e})", report_file.display()))?;
    file.write_all(output.as_bytes())
        .map_err(|e| format!("Failed to write report file {} ({e})", report_file.display()))?;

    Ok(())
}

fn unique_destination_path(dir: &Path, file_name: &OsStr) -> PathBuf {
    let original = PathBuf::from(file_name);
    let stem = original
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("file")
        .to_string();
    let ext = original.extension().and_then(OsStr::to_str).map(str::to_string);

    let mut candidate = dir.join(file_name);
    let mut idx = 1usize;
    while candidate.exists() {
        let renamed = match &ext {
            Some(ext) => format!("{stem}_{idx}.{ext}"),
            None => format!("{stem}_{idx}"),
        };
        candidate = dir.join(renamed);
        idx += 1;
    }

    candidate
}

fn move_file(src: &Path, dst: &Path) -> Result<(), String> {
    match fs::rename(src, dst) {
        Ok(_) => Ok(()),
        Err(_) => {
            fs::copy(src, dst)
                .map_err(|e| format!("Copy failed: {} -> {} ({e})", src.display(), dst.display()))?;
            fs::remove_file(src)
                .map_err(|e| format!("Failed to remove source file {} ({e})", src.display()))
        }
    }
}
