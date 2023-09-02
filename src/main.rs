use clap::Parser;
use env_logger::Env;
use log::{debug, info, warn};
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

    let abs_paths = de_start_with(&args.paths);
    debug!("paths: {:?}", abs_paths);
    let files = get_files_in_folder_recursive(&abs_paths);
    info!("Found {} files", files.len());
    let files = skip_zero_size(files);
    let files = group_hard_links(files);
    let same_size_files = by_size(files);
    let same_digest_files = by_digest(same_size_files);
    for files in same_digest_files {
        debug!("Same size files: {:#?}", files);
    }
}

// Find duplicate paths
fn de_start_with(paths: &Vec<PathBuf>) -> Vec<PathBuf> {
    let mut abs_paths: Vec<PathBuf> = paths
        .iter()
        .map(|path| path.canonicalize().unwrap())
        .collect();
    abs_paths.sort_by(|a, b| a.as_os_str().len().cmp(&b.as_os_str().len()));
    debug!("abs_paths: {:?}", abs_paths);

    let mut result: Vec<PathBuf> = vec![];
    for path in abs_paths {
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

fn skip_zero_size(files: Vec<RegularFile>) -> Vec<RegularFile> {
    let before_length = files.len();
    let result: Vec<RegularFile> = files.into_iter().filter(|f| f.size > 0).collect();
    info!(
        "Skipped {} files with zero size",
        before_length - result.len()
    );
    result
}

fn group_hard_links(files: Vec<RegularFile>) -> Vec<RegularFile> {
    let before_length = files.len();
    let mut result: Vec<RegularFile> = vec![];

    let mut groups: HashMap<(u64, u64), RegularFile> = HashMap::new();
    for f in files {
        let path = f.path.clone();
        groups
            .entry((f.dev, f.ino))
            .or_insert(f)
            .hard_links
            .push(path);
    }

    for (_, f) in groups {
        result.push(f);
    }
    info!("Found {} hard link files", before_length - result.len());

    result
}

fn by_size(files: Vec<RegularFile>) -> Vec<Vec<RegularFile>> {
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
