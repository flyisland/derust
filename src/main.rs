use clap::Parser;
use env_logger::Env;
use log::{debug, info, warn};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

/// Find duplicate files.
#[derive(Parser)]
struct Cli {
    /// The paths or files to look for
    #[arg(required = true)]
    paths: Vec<std::path::PathBuf>,
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
    let mut files: Vec<RegularFile> = files.into_iter().filter(|f| f.size > 0).collect();
    info!(
        "Skipped {} files with zero size",
        files_account - files.len()
    );
    let same_size_files = by_size(&mut files);
    let same_size_files = by_digest(same_size_files);
    for files in same_size_files {
        debug!("Same size files: {:#?}", files);
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

fn get_files_in_folder_recursive(paths: &Vec<PathBuf>) -> Vec<RegularFile> {
    let mut files: Vec<RegularFile> = vec![];
    let mut symbolic_files: Vec<SymbolicFile> = vec![];
    let mut folders: Vec<PathBuf> = paths.clone();

    while let Some(path) = folders.pop() {
        if path.is_symlink() {
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
        } else if path.is_dir() {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        folders.push(entry.path());
                    }
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
        symbolic_files = symbolic_files
            .into_iter()
            .filter(|s| {
                if f.path == s.target {
                    f.symbolic_links.push(s.path.clone());
                    false
                } else {
                    true
                }
            })
            .collect();
    }

    if symbolic_files.len() > 0 {
        warn!(
            "Skipped {} symbolic links not pointing to files in scope",
            symbolic_files.len()
        );
        for s in &symbolic_files {
            debug!("Symbolic link: {:?}->{:?}", s.path, s.target);
        }
    }

    files
}

fn by_size(files: &mut Vec<RegularFile>) -> Vec<Vec<RegularFile>> {
    let files = group_hard_links(files);
    let before_length = files.len();

    let mut result: Vec<Vec<RegularFile>> = vec![];
    let mut groups: HashMap<u64, Vec<RegularFile>> = HashMap::new();
    for f in files {
        groups.entry(f.size).or_insert(vec![]).push(f);
    }
    groups
        .into_iter()
        .filter(|(_, v)| v.len() >= 2)
        .for_each(|(_, v)| {
            result.push(v);
        });
    let after_length = result.iter().fold(0, |acc, f| acc + f.len());

    info!(
        "Skipped {} files with unique size",
        before_length - after_length
    );
    result
}

fn group_hard_links(files: &mut Vec<RegularFile>) -> Vec<RegularFile> {
    let before_length = files.len();
    let mut result: Vec<RegularFile> = vec![];
    files.sort_by(|a, b| {
        if let Ordering::Equal = a.dev.cmp(&b.dev) {
            a.ino.cmp(&b.ino)
        } else {
            a.dev.cmp(&b.dev)
        }
    });

    let mut last_f = files.pop().unwrap();
    while let Some(f) = files.pop() {
        if f.dev == last_f.dev && f.ino == last_f.ino {
            last_f.hard_links.push(f.path.clone());
        } else {
            result.push(last_f);
            last_f = f;
        }
    }
    result.push(last_f);
    info!("Found {} hard link files", before_length - result.len());

    result
}

fn get_md5(path: &PathBuf) -> Vec<u8> {
    let mut file = fs::File::open(path).unwrap();
    let mut buffer = Vec::new();
    let _ = file.read_to_end(&mut buffer);

    let digest = md5::compute(buffer);
    digest.to_ascii_lowercase()
}

fn by_digest(files: Vec<Vec<RegularFile>>) -> Vec<Vec<RegularFile>> {
    let before_length = files.iter().fold(0, |acc, f| acc + f.len());
    let mut result: Vec<Vec<RegularFile>> = vec![];

    for same_size_files in files {
        let mut groups: HashMap<Vec<u8>, Vec<RegularFile>> = HashMap::new();
        for f in same_size_files {
            groups.entry(get_md5(&f.path)).or_insert(vec![]).push(f);
        }
        groups
            .into_iter()
            .filter(|(_, v)| v.len() >= 2)
            .for_each(|(_, v)| {
                result.push(v);
            });
    }
    let after_length = result.iter().fold(0, |acc, f| acc + f.len());
    info!(
        "Skipped {} files with unique digest",
        before_length - after_length
    );

    result
}
