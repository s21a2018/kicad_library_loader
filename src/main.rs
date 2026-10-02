use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::mpsc,
};
use zip::ZipArchive;

const LIBRARY_EXTENSIONS: &[&str] = &["kicad_sym", "kicad_mod", "kicad_wks", "pretty"];

#[derive(Debug)]
struct Options {
    source: PathBuf,
    destination: PathBuf,
    once: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::from_args()?;
    fs::create_dir_all(&options.destination)?;
    println!(
        "KiCad library destination: {}",
        options.destination.display()
    );
    import_existing(&options.source, &options.destination)?;

    if options.once {
        return Ok(());
    }

    let (sender, receiver) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(sender, Config::default())?;
    watcher.watch(&options.source, RecursiveMode::NonRecursive)?;
    println!("Watching Downloads: {}", options.source.display());

    for result in receiver {
        match result {
            Ok(event) if is_relevant_event(&event) => {
                for path in event.paths {
                    if let Err(error) = import_path(&path, &options.destination) {
                        eprintln!("Could not import {}: {error}", path.display());
                    }
                }
            }
            Ok(_) => {}
            Err(error) => eprintln!("Watcher error: {error}"),
        }
    }
    Ok(())
}

impl Options {
    fn from_args() -> Result<Self, Box<dyn Error>> {
        let mut source = default_downloads_dir()?;
        let mut destination = default_library_dir()?;
        let mut once = false;
        let args: Vec<String> = env::args().skip(1).collect();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--once" => once = true,
                "--source" => source = next_path(&args, &mut index, "--source")?,
                "--destination" => destination = next_path(&args, &mut index, "--destination")?,
                "--help" | "-h" => {
                    println!(
                        "Usage: kicad_library_loader [--once] [--source PATH] [--destination PATH]"
                    );
                    std::process::exit(0);
                }
                argument => return Err(format!("unknown argument: {argument}").into()),
            }
            index += 1;
        }
        if !source.is_dir() {
            return Err(format!("source directory does not exist: {}", source.display()).into());
        }
        Ok(Self {
            source,
            destination,
            once,
        })
    }
}

fn next_path(args: &[String], index: &mut usize, option: &str) -> Result<PathBuf, Box<dyn Error>> {
    *index += 1;
    args.get(*index)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{option} requires a path").into())
}

fn default_downloads_dir() -> Result<PathBuf, Box<dyn Error>> {
    let profile = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .ok_or("USERPROFILE or HOME is not set")?;
    Ok(PathBuf::from(profile).join("Downloads"))
}

fn default_library_dir() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = env::var_os("KICAD_USER_LIB_DIR") {
        return Ok(PathBuf::from(path));
    }
    let profile = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .ok_or("USERPROFILE or HOME is not set")?;
    Ok(PathBuf::from(profile)
        .join("Documents")
        .join("KiCad")
        .join("libraries"))
}

fn import_existing(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(source)? {
        let path = entry?.path();
        if is_supported_path(&path) {
            import_path(&path, destination)?;
        }
    }
    repair_library_tables(destination)
}

fn is_relevant_event(event: &Event) -> bool {
    matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_))
}

fn is_supported_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            LIBRARY_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
                || extension.eq_ignore_ascii_case("zip")
        })
}

fn import_path(path: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    if !path.exists() || !is_supported_path(path) {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    if path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        extract_archive(path, destination)?;
    } else {
        let target = destination.join(
            path.file_name()
                .ok_or_else(|| format!("path has no file name: {}", path.display()))?,
        );
        if path.is_dir() {
            copy_directory(path, &target)?;
        } else {
            fs::copy(path, &target)?;
        }
        println!("Imported {}", target.display());
    }
    repair_library_tables(destination)
}

fn extract_archive(archive_path: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("unsafe path in archive: {}", entry.name()))?
            .to_owned();
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&target)?;
        io::copy(&mut entry, &mut output)?;
        println!("Imported {}", target.display());
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn repair_library_tables(destination: &Path) -> Result<(), Box<dyn Error>> {
    let uri_pattern = Regex::new(r#"\(uri\s+"([^"]+)""#)?;
    for table_name in ["sym-lib-table", "fp-lib-table"] {
        let table = destination.join(table_name);
        if !table.is_file() {
            continue;
        }
        let mut contents = String::new();
        File::open(&table)?.read_to_string(&mut contents)?;
        let table_dir = table.parent().ok_or("table has no parent directory")?;
        let repaired = uri_pattern.replace_all(&contents, |captures: &regex::Captures| {
            let uri = &captures[1];
            if uri.contains("${") || Path::new(uri).is_absolute() {
                return captures[0].to_owned();
            }
            let absolute = table_dir.join(uri);
            if absolute.exists() {
                format!(
                    r#"(uri "{}""#,
                    absolute.to_string_lossy().replace('\\', "/")
                )
            } else {
                captures[0].to_owned()
            }
        });
        if repaired != contents {
            fs::write(&table, repaired.as_ref())?;
            println!("Repaired library paths in {}", table.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn repairs_relative_library_uri() {
        let root = env::temp_dir().join(format!(
            "kicad-library-loader-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("symbols.kicad_sym"), "(kicad_symbol_lib)").unwrap();
        fs::write(
            root.join("sym-lib-table"),
            "(sym_lib_table (lib (name \"local\")(uri \"symbols.kicad_sym\")))",
        )
        .unwrap();

        repair_library_tables(&root).unwrap();
        let contents = fs::read_to_string(root.join("sym-lib-table")).unwrap();
        assert!(contents.contains("uri \""));
        assert!(contents.contains("symbols.kicad_sym"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_unsupported_files() {
        assert!(!is_supported_path(Path::new("notes.txt")));
        assert!(is_supported_path(Path::new("library.zip")));
    }
}
