use std::env;
use std::fs;
use std::io::{self, Write};
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

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = if a[i - 1] == b[j - 1] {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j - 1])
            };
            prev = cur;
        }
    }
    row[b.len()]
}

fn name_of(p: &Path) -> String {
    p.file_stem().unwrap_or_default().to_string_lossy().to_lowercase()
}

fn aliases_path() -> Option<PathBuf> {
    Some(PathBuf::from(env::var("APPDATA").ok()?).join(r"calamitus\aliases.txt"))
}

/// Loads user-defined `name=target` aliases, for apps the Start Menu scan can't find
/// (e.g. Store/UWP apps like WhatsApp, which use a `shell:AppsFolder\...` target instead of a .lnk).
fn load_aliases() -> Vec<(String, String)> {
    let Some(path) = aliases_path() else { return vec![] };
    let Ok(contents) = fs::read_to_string(&path) else {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).ok();
        }
        fs::write(
            &path,
            "# map names to launch targets, one per line: name=target\n\
             # target can be a .lnk path, an .exe path, or shell:AppsFolder\\<id> for Store apps\n\
             # (find Store app ids with: powershell -Command \"Get-StartApps\")\n\
             # whatsapp=shell:AppsFolder\\5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App\n",
        ).ok();
        return vec![];
    };
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, target) = line.split_once('=')?;
            Some((name.trim().to_lowercase(), target.trim().to_string()))
        })
        .collect()
}

/// Prompts the user to pick one of `candidates` by number. Returns the chosen path, or exits on bad input.
fn prompt_choice<'a>(header: &str, candidates: &[&'a PathBuf]) -> &'a PathBuf {
    eprintln!("{header}");
    for (i, c) in candidates.iter().enumerate() {
        eprintln!("  {}) {}", i + 1, name_of(c));
    }
    eprint!("select [1-{}]: ", candidates.len());
    io::stderr().flush().ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        std::process::exit(1);
    }
    match input.trim().parse::<usize>() {
        Ok(n) if n >= 1 && n <= candidates.len() => candidates[n - 1],
        _ => {
            eprintln!("invalid selection");
            std::process::exit(1);
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

    if let Some((_, target)) = load_aliases().into_iter().find(|(name, _)| *name == query) {
        launch(&PathBuf::from(target));
        return;
    }

    let mut shortcuts = vec![];
    for dir in start_menu_dirs() {
        collect_shortcuts(&dir, &mut shortcuts);
    }
    if shortcuts.is_empty() {
        eprintln!("no Start Menu shortcuts found");
        std::process::exit(1);
    }

    if let Some(exact) = shortcuts.iter().find(|p| name_of(p) == query) {
        launch(exact);
        return;
    }

    let substring_matches: Vec<&PathBuf> = shortcuts.iter().filter(|p| name_of(p).contains(&query)).collect();
    let chosen = match substring_matches.as_slice() {
        [only] => only,
        [] => {
            // no exact/substring hit: fall back to closest names by edit distance
            let mut by_distance: Vec<&PathBuf> = shortcuts.iter().collect();
            by_distance.sort_by_key(|p| levenshtein(&name_of(p), &query));
            by_distance.truncate(5);
            prompt_choice(&format!("no app found matching '{query}', did you mean:"), &by_distance)
        }
        many => prompt_choice(&format!("multiple matches for '{query}':"), many),
    };
    launch(chosen);
}

fn launch(shortcut: &Path) {
    if let Err(e) = Command::new("explorer").arg(shortcut).spawn() {
        eprintln!("failed to launch {}: {e}", shortcut.display());
        std::process::exit(1);
    }
}
