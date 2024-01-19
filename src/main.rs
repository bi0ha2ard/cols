use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use itertools::Itertools;
use serde::Deserialize;
use serde_xml_rs::from_reader;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};

fn default_build_type() -> String {
    "ros.catkin".to_string()
}

fn default_export() -> Export {
    Export {
        build_type: default_build_type(),
    }
}

#[derive(Deserialize)]
struct Export {
    #[serde(default = "default_build_type")]
    build_type: String,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    #[serde(default = "default_export")]
    export: Export,
}

struct Entry {
    pkg: Package,
    path: PathBuf,
}

impl Entry {
    fn print(&self) {
        println!(
            "{}\t{}\t({})",
            self.pkg.name,
            &self.path.to_string_lossy(),
            self.pkg.export.build_type,
        );
    }

    fn print_from_opts(&self, args: &ListArgs) {
        if args.names_only {
            println!("{}", self.pkg.name);
        } else if args.paths_only {
            println!("{}", self.path.to_string_lossy());
        } else {
            self.print();
        }
    }
}

enum SearchOutcome {
    Found(Entry),
    Ignored,
    IsFile,
    Recurse,
}

static IGNORE_MARKERS: [&str; 3] = ["COLCON_IGNORE", "CATKIN_IGNORE", "AMENT_IGNORE"];

// TODO: follow symlinks?
fn check_path(dir: &Path) -> SearchOutcome {
    use SearchOutcome::*;
    if !dir.is_dir() {
        return SearchOutcome::IsFile {};
    }

    let is_dot_file = dir
        .file_name()
        .map(|x| x.to_string_lossy())
        .map(|x| x.starts_with('.'));
    if let Some(true) = is_dot_file {
        return Ignored {};
    }

    if IGNORE_MARKERS
        .iter()
        .any(|ignore| dir.join(ignore).exists())
    {
        return Ignored {};
    }

    let pkg_xml = dir.join("package.xml");
    if pkg_xml.exists() {
        match parse_package(&pkg_xml) {
            Ok(pkg) => {
                return Found(Entry {
                    pkg,
                    path: dir.to_path_buf(),
                });
            }
            Err(_e) => {
                // eprintln!("Could not parse '{:?}': {}", pkg_xml, _e);
            }
        }
    }
    Recurse {}
}

fn parse_package(xml_file: &PathBuf) -> Result<Package> {
    let f = File::open(xml_file)?;
    let reader = BufReader::new(f);
    let p: Package = from_reader(reader)?;
    Ok(p)
}

fn find_packages(
    dir: &Path,
    results: &mut Vec<Entry>,
    recurse: bool,
) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    use SearchOutcome::*;
    for entry in (fs::read_dir(dir)?).flatten() {
        let check_outcome = check_path(&entry.path());
        match check_outcome {
            Found(entry) => {
                results.push(entry);
            }
            Recurse if recurse => {
                find_packages(&entry.path(), results, recurse)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Fast colcon list replacement
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct MainArgs {
    /// Unused, we don't log anything
    #[arg(long)]
    log_base: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Args)]
struct ListArgs {
    /// Not implemented
    #[arg(short = 't', long, default_value_t = false)]
    topological_order: bool,

    /// Output only the name of each package but not the path
    #[arg(
        short = 'n',
        long,
        default_value_t = false,
        conflicts_with = "paths_only"
    )]
    names_only: bool,

    /// Output only the path of each package but not the name
    #[arg(
        short = 'p',
        long,
        default_value_t = false,
        conflicts_with = "names_only"
    )]
    paths_only: bool,

    /// The base paths to recursively crawl for packages
    #[arg(long, num_args = 0..)]
    base_paths: Vec<std::path::PathBuf>,

    /// The paths to check for a package. Use shell wildcards (e.g. `src/*`) to select all direct subdirectories
    /// TODO: we don't do globs yet
    #[arg(long, num_args = 0..)]
    paths: Vec<std::path::PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// List packages, optionally in topological ordering.
    List(ListArgs),
}

fn preprocess_paths(raw: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    return raw
        .iter()
        .dedup()
        .map(|p| p.canonicalize().unwrap_or(p.clone()))
        .dedup()
        .collect();
}

fn main() -> io::Result<()> {
    let args = MainArgs::parse();
    match &args.command {
        Commands::List(list_args) => {
            let mut res = Vec::<Entry>::new();

            let unique_paths = preprocess_paths(&list_args.paths);
            for to_check in &unique_paths {
                find_packages(to_check, &mut res, false)?;
            }

            if unique_paths.is_empty() && list_args.base_paths.is_empty() {
                find_packages(Path::new("."), &mut res, true)?;
            } else {
                for p in preprocess_paths(&list_args.base_paths) {
                    find_packages(&p, &mut res, true)?;
                }
            }

            for e in res
                .iter()
                .sorted_unstable_by_key(|e| (&e.pkg.name, &e.path))
                .dedup_by(|a, b| a.pkg.name == b.pkg.name && a.path == b.path)
            {
                e.print_from_opts(list_args);
            }
        }
    }
    Ok(())
}
