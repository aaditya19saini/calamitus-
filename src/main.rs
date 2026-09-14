use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn start_menu_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![];
    if let Ok(appdata) = env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Ok(programdata) = env::var("ProgramData") {
        dirs.push(PathBuf::from(programdata).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    dirs
}

fn collect_shortcuts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_shortcuts(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
            out.push(path);
        }
    }
}

fn main() {
    let query = env::args().skip(1).collect::<Vec<_>>().join(" ");
    if query.is_empty() {
        eprintln!("usage: launch <app name>");
        std::process::exit(1);
    }
    let query = query.to_lowercase();

    let mut shortcuts = vec![];
    for dir in start_menu_dirs() {
        collect_shortcuts(&dir, &mut shortcuts);
    }

    let name_of = |p: &Path| -> String {
        p.file_stem().unwrap_or_default().to_string_lossy().to_lowercase()
    };

    if let Some(exact) = shortcuts.iter().find(|p| name_of(p) == query) {
        launch(exact);
        return;
    }

    let matches: Vec<&PathBuf> = shortcuts.iter().filter(|p| name_of(p).contains(&query)).collect();
    match matches.as_slice() {
        [] => {
            eprintln!("no app found matching '{query}'");
            std::process::exit(1);
        }
        [only] => launch(only),
        many => {
            eprintln!("multiple matches for '{query}', be more specific:");
            for m in many {
                eprintln!("  {}", name_of(m));
            }
            std::process::exit(1);
        }
    }
}

fn launch(shortcut: &Path) {
    if let Err(e) = Command::new("explorer").arg(shortcut).spawn() {
        eprintln!("failed to launch {}: {e}", shortcut.display());
        std::process::exit(1);
    }
}
