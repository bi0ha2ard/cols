use anyhow::Result;
use serde::Deserialize;
use serde_xml_rs::from_reader;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Export {
    build_type: String,
}

#[derive(Deserialize)]
struct Package {
    name: String,
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
}

enum SearchOutcome {
    Found { entry: Entry },
    Ignored,
    IsFile,
    Recurse,
}

static IGNORE_MARKERS: [&str; 3] = ["COLCON_IGNORE", "CATKIN_IGNORE", "AMENT_IGNORE"];

fn check_path(dir: &PathBuf) -> SearchOutcome {
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
        if let Ok(pkg) = parse_package(&pkg_xml) {
            return Found {
                entry: Entry {
                    pkg,
                    path: dir.clone(),
                },
            };
        }
    }
    return Recurse {};
}

fn parse_package(xml_file: &PathBuf) -> Result<Package> {
    let f = File::open(xml_file)?;
    let reader = BufReader::new(f);
    let p: Package = from_reader(reader)?;
    return Ok(p);
}

fn find_packages(dir: &Path, results: &mut Vec<Entry>, immediate: bool) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    use SearchOutcome::*;
    for entry in fs::read_dir(dir)? {
        if let Ok(e) = entry {
            let check_outcome = check_path(&e.path());
            match check_outcome {
                Found { entry: pkg } => {
                    if immediate {
                        pkg.print();
                    } else {
                        results.push(pkg);
                    }
                }
                Recurse => {
                    find_packages(&e.path(), results, immediate)?;
                }
                _ => {}
            }
        }
    }
    return Ok(());
}

fn main() -> io::Result<()> {
    let immediate = true;
    let mut res = Vec::<Entry>::new();
    find_packages(Path::new("."), &mut res, immediate)?;
    if !immediate {
        res.sort_unstable_by(|a, b| return a.pkg.name.cmp(&b.pkg.name));
        for e in res {
            e.print();
        }
    }
    return Ok(());
}
