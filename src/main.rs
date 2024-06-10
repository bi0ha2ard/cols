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

fn find_packages(dir: &Path, results: &mut Vec<Entry>, recurse: bool) -> io::Result<()> {
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

fn find_wrapper(dir: &Path, results: &mut Vec<Entry>, recurse: bool) -> io::Result<()> {
    if let SearchOutcome::Found(entry) = check_path(dir) {
        results.push(entry);
    }
    find_packages(dir, results, recurse)?;
    Ok(())
}

fn preprocess_paths(raw: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    return raw
        .iter()
        .dedup()
        .map(|p| p.canonicalize().unwrap_or(p.clone()))
        .dedup()
        .collect();
}

fn collect_packages_from_args(
    raw_paths: &[std::path::PathBuf],
    base_paths: &[std::path::PathBuf],
) -> io::Result<Vec<Entry>> {
    let mut res = Vec::<Entry>::new();

    let unique_paths = preprocess_paths(raw_paths);
    for to_check in &unique_paths {
        find_wrapper(to_check, &mut res, false)?;
    }

    if unique_paths.is_empty() && base_paths.is_empty() {
        find_wrapper(Path::new("."), &mut res, true)?;
    } else {
        preprocess_paths(base_paths)
            .into_iter()
            .map(|p| find_wrapper(&p, &mut res, true))
            .collect::<io::Result<Vec<_>>>()?;
    }
    Ok(res)
}

macro_rules! print_unless_quiet {
    ($i:ident, $($arg:tt)*) => {
        if !$i {
            println!($($arg)*);
        }
    };
}

fn try_symlink(entry: &Entry, build_base: std::path::PathBuf, quiet: bool, force: bool) {
    let mut cmake_file = entry.path.clone();
    cmake_file.push("CMakeLists.txt");
    if !cmake_file.exists() {
        print_unless_quiet!(
            quiet,
            "[INFO] Skipping {} because no CMakeLists.txt was found @ {}.",
            entry.pkg.name,
            cmake_file.to_string_lossy()
        );
        return;
    }
    let mut link_in_src_space = cmake_file;
    link_in_src_space.pop();
    link_in_src_space.push("compile_commands.json");
    let mut build_base = build_base;
    build_base.push(entry.pkg.name.clone());
    build_base.push("compile_commands.json");

    if link_in_src_space.is_symlink() && force {
        print_unless_quiet!(
            quiet,
            "[WARNING] Removing existing symlink in {}",
            link_in_src_space.to_string_lossy()
        );
        if let Err(e) = std::fs::remove_file(&link_in_src_space) {
            print_unless_quiet!(quiet, "[ERROR] Couldn't remove symlink: {}", e);
            return;
        }
    }

    if let Err(e) = std::os::unix::fs::symlink(&build_base, &link_in_src_space) {
        print_unless_quiet!(quiet, "[ERROR] Couldn't create symlink: {}", e);
        return;
    }
    print_unless_quiet!(
        quiet,
        "[INFO] Created link for {} from {} -> {}",
        entry.pkg.name,
        link_in_src_space.to_string_lossy(),
        build_base.to_string_lossy()
    );
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

#[derive(Args)]
struct SymlinkArgs {
    /// The base paths to recursively crawl for packages
    #[arg(long, num_args = 0..)]
    base_paths: Vec<std::path::PathBuf>,

    /// The paths to check for a package. Use shell wildcards (e.g. `src/*`) to select all direct subdirectories
    /// TODO: we don't do globs yet
    #[arg(long, num_args = 0..)]
    paths: Vec<std::path::PathBuf>,

    /// The base path for all build directories
    #[arg(long)]
    build_base: std::path::PathBuf,

    /// Don't print anything
    #[arg(short = 'q', long, default_value_t = false)]
    quiet: bool,

    /// Whether to re-create existing links
    #[arg(short = 'f', long, default_value_t = false)]
    force: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// List packages, optionally in topological ordering.
    List(ListArgs),

    /// (Non-standard) Creates symlinks from src/<pkg>/compile_commands.json -> build/<pkg>/compile_commands.json
    Symlink(SymlinkArgs),
}

fn rel_to_cwd(build_base: PathBuf) -> PathBuf {
    match std::env::current_dir() {
        Ok(mut d) => {
            d.push(build_base);
            d
        }
        Err(_) => build_base,
    }
}

fn main() -> io::Result<()> {
    let args = MainArgs::parse();
    match &args.command {
        Commands::List(list_args) => {
            let res = collect_packages_from_args(&list_args.paths, &list_args.base_paths)?;

            for e in res
                .iter()
                .sorted_unstable_by_key(|e| (&e.pkg.name, &e.path))
                .dedup_by(|a, b| a.pkg.name == b.pkg.name && a.path == b.path)
            {
                e.print_from_opts(list_args);
            }
        }
        Commands::Symlink(symlink_args) => {
            let res = collect_packages_from_args(&symlink_args.paths, &symlink_args.base_paths)?;
            let fixed_build = symlink_args
                .build_base
                .canonicalize()
                .unwrap_or(rel_to_cwd(symlink_args.build_base.clone()));
            for e in res {
                try_symlink(
                    &e,
                    fixed_build.clone(),
                    symlink_args.quiet,
                    symlink_args.force,
                );
            }
        }
    }
    Ok(())
}
