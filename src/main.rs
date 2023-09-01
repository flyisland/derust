#![feature(absolute_path)]

use clap::Parser;
use env_logger::Env;
use log::{debug, info, warn};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

/// Find duplicate files.
#[derive(Parser)]
struct Cli {
    /// The paths or files to look for
    #[arg(required = true)]
    paths: Vec<std::path::PathBuf>,
}

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp(None)
        //        .format_level(false)
        .format_target(false)
        .init();
    let args = Cli::parse();
    info!("Now scanning {:?} ...", args.paths);

    let mut abs_paths: Vec<PathBuf> = args
        .paths
        .iter()
        .map(|path| path.canonicalize().unwrap())
        .collect();
    abs_paths.sort_by(|a, b| a.as_os_str().len().cmp(&b.as_os_str().len()));
    debug!("abs_paths: {:?}", abs_paths);
    let paths = de_start_with(&abs_paths);
    debug!("paths: {:?}", paths);
    let files = get_files_in_folder_recursive(&paths);
    let files_account = files.len();
    info!("Found {} files", files_account);
    let files: Vec<&RegularFile> = files.iter().filter(|f| f.size > 0).collect();
    let zero_size_files = files_account - files.len();
    info!("Skipped {} files with zero size", zero_size_files);

    for f in files {
        println!("{:?}", f);
    }
}

// Find duplicate paths
fn de_start_with(paths: &Vec<std::path::PathBuf>) -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = vec![];
    for path in paths {
        if let Some(start_with) = result.iter().find(|prefix| path.starts_with(prefix)) {
            warn!(
                "Skip path: \"{}\" starts with \"{}\"",
                path.display(),
                start_with.display()
            );
        } else {
            result.push(path.clone());
        }
    }

    result
}

#[derive(Debug)]
struct SymbolicFile {
    path: PathBuf,
    target: PathBuf,
}

#[derive(Debug)]
struct RegularFile {
    path: PathBuf,
    size: u64,
    dev: u64,
    ino: u64,
    hard_links: Vec<PathBuf>,
    symbolic_links: Vec<PathBuf>,
}

fn get_files_in_folder_recursive(paths: &Vec<PathBuf>) -> Vec<RegularFile> {
    let mut files: Vec<RegularFile> = vec![];
    let mut symbolic_files: Vec<SymbolicFile> = vec![];
    let mut folders: Vec<PathBuf> = paths.clone();

    while let Some(path) = folders.pop() {
        if path.is_dir() {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        folders.push(entry.path());
                    }
                }
            }
        } else if path.is_symlink() {
            match path.canonicalize() {
                Ok(target) => symbolic_files.push(SymbolicFile {
                    path: path.clone(),
                    target,
                }),
                Err(err) => {
                    warn!(
                        "Failed to canonicalize symlink: {:?}->{:?}, error: {:?}",
                        path,
                        path.read_link().unwrap(),
                        err
                    );
                }
            }
        } else {
            let metadata = path.metadata().unwrap();
            files.push(RegularFile {
                path,
                size: metadata.size(),
                dev: metadata.dev(),
                ino: metadata.ino(),
                hard_links: vec![],
                symbolic_links: vec![],
            });
        }
    }

    for f in &mut files {
        for s in &symbolic_files {
            if f.path == s.target {
                f.symbolic_links.push(s.path.clone());
            }
        }
    }

    files
}
