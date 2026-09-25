use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppTarget {
    Shortcut(PathBuf),
    Alias { name: String, target: String },
}

impl AppTarget {
    pub fn name(&self) -> String {
        match self {
            AppTarget::Shortcut(p) => name_of(p),
            AppTarget::Alias { name, .. } => name.clone(),
        }
    }

    pub fn display(&self) -> String {
        match self {
            AppTarget::Shortcut(p) => name_of(p),
            AppTarget::Alias { name, target } => format!("{name} (alias -> {target})"),
        }
    }

    pub fn launch(&self) {
        let status = match self {
            AppTarget::Shortcut(p) => Command::new("explorer").arg(p).spawn(),
            AppTarget::Alias { target, .. } => Command::new("explorer").arg(target).spawn(),
        };
        if let Err(e) = status {
            eprintln!("failed to launch {}: {e}", self.display());
            std::process::exit(1);
        }
    }
}

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
        } else if path.extension().is_some_and(|e| {
            e.eq_ignore_ascii_case("lnk") || e.eq_ignore_ascii_case("url")
        }) {
            out.push(path);
        }
    }
}

pub fn levenshtein(a: &str, b: &str) -> usize {
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

pub fn name_of(p: &Path) -> String {
    p.file_stem().unwrap_or_default().to_string_lossy().to_lowercase()
}

// every substring is a subsequence, so this also catches abbreviations
// like "vs code" -> "visual studio code" that plain substring matching misses.
pub fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut hay = haystack.chars();
    needle.chars().all(|c| hay.any(|h| h == c))
}

pub fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn aliases_path() -> Option<PathBuf> {
    Some(PathBuf::from(env::var("APPDATA").ok()?).join(r"calamitus\aliases.txt"))
}

/// Loads user-defined `name=target` aliases, for apps the Start Menu scan can't find
/// (e.g. Store/UWP apps like WhatsApp, which use a `shell:AppsFolder\...` target instead of a .lnk).
pub fn load_aliases() -> Vec<(String, String)> {
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
    parse_aliases(&contents)
}

pub fn parse_aliases(contents: &str) -> Vec<(String, String)> {
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

/// Finds matching app targets using tiered search strategy:
/// Tier 1: Exact name match
/// Tier 2: Prefix or word-prefix match
/// Tier 3: Substring match
/// Tier 4: Subsequence match
pub fn find_matches<'a>(candidates: &'a [AppTarget], query: &str) -> Vec<&'a AppTarget> {
    let query_compact = compact(query);

    // Tier 1: Exact match
    let exact: Vec<&AppTarget> = candidates.iter().filter(|c| c.name() == query).collect();
    if !exact.is_empty() {
        return exact;
    }

    // Tier 2: Prefix or word-prefix match
    let prefix: Vec<&AppTarget> = candidates
        .iter()
        .filter(|c| {
            let name = c.name();
            name.starts_with(query) || name.split_whitespace().any(|w| w.starts_with(query))
        })
        .collect();
    if !prefix.is_empty() {
        return prefix;
    }

    // Tier 3: Substring match
    let substring: Vec<&AppTarget> = candidates
        .iter()
        .filter(|c| c.name().contains(query))
        .collect();
    if !substring.is_empty() {
        return substring;
    }

    // Tier 4: Subsequence match
    let subsequence: Vec<&AppTarget> = candidates
        .iter()
        .filter(|c| is_subsequence(&query_compact, &compact(&c.name())))
        .collect();
    if !subsequence.is_empty() {
        return subsequence;
    }

    vec![]
}

/// Prompts the user to pick one of `candidates` by number. Returns the chosen target, or exits on bad input.
fn prompt_choice<'a>(header: &str, candidates: &[&'a AppTarget]) -> &'a AppTarget {
    eprintln!("{header}");
    for (i, c) in candidates.iter().enumerate() {
        eprintln!("  {}) {}", i + 1, c.display());
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

fn load_all_candidates() -> Vec<AppTarget> {
    let mut candidates: Vec<AppTarget> = Vec::new();
    let mut seen_names = HashSet::new();

    for (alias_name, target) in load_aliases() {
        if seen_names.insert(alias_name.clone()) {
            candidates.push(AppTarget::Alias { name: alias_name, target });
        }
    }

    let mut shortcut_paths = vec![];
    for dir in start_menu_dirs() {
        collect_shortcuts(&dir, &mut shortcut_paths);
    }

    for path in shortcut_paths {
        let name = name_of(&path);
        if seen_names.insert(name) {
            candidates.push(AppTarget::Shortcut(path));
        }
    }

    candidates
}

fn print_help() {
    println!("calamitus / launch v{}", env!("CARGO_PKG_VERSION"));
    println!("Open apps by typing their name in a terminal.\n");
    println!("USAGE:");
    println!("    launch <app name>          Launch app matching the given name");
    println!("    launch --help, -h          Print help information");
    println!("    launch --version, -v       Print version information");
    println!("    launch --list, -l          List all available shortcuts and aliases");
    println!("    launch --aliases, -a       Show path or open alias config file\n");
    println!("EXAMPLES:");
    println!("    launch chrome");
    println!("    launch vs code");
    println!("    launch spotify");
}

fn handle_aliases_cmd() {
    load_aliases(); // ensures file exists
    if let Some(path) = aliases_path() {
        println!("Alias configuration file path:");
        println!("  {}", path.display());
        Command::new("explorer").arg(&path).spawn().ok();
    } else {
        eprintln!("Could not determine APPDATA directory.");
        std::process::exit(1);
    }
}

fn handle_list_cmd() {
    let candidates = load_all_candidates();
    if candidates.is_empty() {
        println!("No Start Menu shortcuts or aliases found.");
        return;
    }
    println!("Available applications ({}) :", candidates.len());
    let mut sorted = candidates;
    sorted.sort_by_key(|c| c.name());
    for candidate in sorted {
        println!("  - {}", candidate.display());
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        std::process::exit(1);
    }

    if args.len() == 1 {
        match args[0].as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--version" | "-v" => {
                println!("launch {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--list" | "-l" => {
                handle_list_cmd();
                return;
            }
            "--aliases" | "-a" => {
                handle_aliases_cmd();
                return;
            }
            _ => {}
        }
    }

    // single-word queries that are already runnable via PATH (git, code, notepad, ...)
    // get launched directly, skipping Start Menu matching entirely.
    if args.len() == 1 && Command::new(&args[0]).spawn().is_ok() {
        return;
    }

    let query = args.join(" ").to_lowercase();
    let candidates = load_all_candidates();

    if candidates.is_empty() {
        eprintln!("no Start Menu shortcuts or aliases found");
        std::process::exit(1);
    }

    let matches = find_matches(&candidates, &query);
    let chosen = match matches.as_slice() {
        [only] => only,
        [] => {
            // no match in any tier: fall back to closest names by edit distance
            let mut by_distance: Vec<&AppTarget> = candidates.iter().collect();
            by_distance.sort_by_key(|c| levenshtein(&c.name(), &query));
            by_distance.truncate(5);
            prompt_choice(&format!("no app found matching '{query}', did you mean:"), &by_distance)
        }
        many => prompt_choice(&format!("multiple matches for '{query}':"), many),
    };

    chosen.launch();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("chrome", "chrome"), 0);
        assert_eq!(levenshtein("code", "vscode"), 2);
    }

    #[test]
    fn test_is_subsequence() {
        assert!(is_subsequence("vscode", "visual studio code"));
        assert!(is_subsequence("chr", "chrome"));
        assert!(!is_subsequence("xyz", "chrome"));
    }

    #[test]
    fn test_compact() {
        assert_eq!(compact(" vs code "), "vscode");
        assert_eq!(compact("Google Chrome"), "GoogleChrome");
    }

    #[test]
    fn test_parse_aliases() {
        let content = "
# Comment line
whatsapp=shell:AppsFolder\\5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App
  custom = C:\\Tools\\CustomApp.exe
invalidline
";
        let aliases = parse_aliases(content);
        assert_eq!(aliases.len(), 2);
        assert_eq!(
            aliases[0],
            (
                "whatsapp".to_string(),
                "shell:AppsFolder\\5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App".to_string()
            )
        );
        assert_eq!(
            aliases[1],
            ("custom".to_string(), "C:\\Tools\\CustomApp.exe".to_string())
        );
    }

    #[test]
    fn test_find_matches_tiers() {
        let candidates = vec![
            AppTarget::Shortcut(PathBuf::from("Google Chrome.lnk")),
            AppTarget::Shortcut(PathBuf::from("Visual Studio Code.lnk")),
            AppTarget::Shortcut(PathBuf::from("Notepad.lnk")),
            AppTarget::Alias {
                name: "vsc".to_string(),
                target: "code.exe".to_string(),
            },
        ];

        // Exact match
        let res = find_matches(&candidates, "notepad");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].name(), "notepad");

        // Prefix match ("code" -> word prefix match for "visual studio code")
        let res = find_matches(&candidates, "code");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].name(), "visual studio code");

        // Substring match
        let res = find_matches(&candidates, "studio");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].name(), "visual studio code");

        // Subsequence match ("vsc" exact match tier 1 via alias)
        let res = find_matches(&candidates, "vsc");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].name(), "vsc");
    }
}
