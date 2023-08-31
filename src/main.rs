#![feature(absolute_path)]

use clap::Parser;
use env_logger::Env;
use log::{debug, info, warn};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
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
        .map(|path| std::path::absolute(path).unwrap())
        .collect();
    abs_paths.sort_by(|a, b| a.as_os_str().len().cmp(&b.as_os_str().len()));
    debug!("abs_paths: {:?}", abs_paths);
    let paths = de_start_with(&abs_paths);
    debug!("paths: {:?}", paths);
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

fn try_exists(paths: &Vec<std::path::PathBuf>) {
    paths.iter().for_each(|path| {
        let meta = fs::metadata(path).unwrap();
        println!("dev:{}, ino:{}, {}", meta.dev(), meta.ino(), path.display(),);
    })
}

fn get_files_in_folder_recursive(folder_path: &Path) -> Vec<PathBuf> {
    let mut file_paths: Vec<PathBuf> = vec![];

    if let Ok(entries) = fs::read_dir(folder_path) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();

                if path.is_file() {
                    file_paths.push(path);
                } else if path.is_dir() {
                    let subfolder_files = get_files_in_folder_recursive(&path);
                    file_paths.extend(subfolder_files);
                }
            }
        }
    }

    file_paths
}
