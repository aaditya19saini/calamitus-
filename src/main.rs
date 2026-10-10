use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Something `launch` can open: a lowercased display name and what to hand to explorer.
struct App {
    name: String,
    target: PathBuf,
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

fn collect_shortcuts(dir: &Path, out: &mut Vec<App>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_shortcuts(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
            out.push(App { name: name_of(&path), target: path });
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

// every substring is a subsequence, so this also catches abbreviations
// like "vs code" -> "visual studio code" that plain substring matching misses.
fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut hay = haystack.chars();
    needle.chars().all(|c| hay.any(|h| h == c))
}

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Apps matching `query`, from the tightest tier that has any: exact name,
/// then substring, then subsequence. Shortest names first, same-named duplicates dropped.
fn find_matches<'a>(query: &str, apps: &'a [App]) -> Vec<&'a App> {
    let query_compact = compact(query);
    let tiers: [&dyn Fn(&str) -> bool; 3] = [
        &|name| name == query,
        &|name| compact(name).contains(&query_compact),
        &|name| is_subsequence(&query_compact, &compact(name)),
    ];
    for tier in tiers {
        let mut hits: Vec<&App> = apps.iter().filter(|a| tier(&a.name)).collect();
        if !hits.is_empty() {
            hits.sort_by(|a, b| (a.name.len(), &a.name).cmp(&(b.name.len(), &b.name)));
            hits.dedup_by(|a, b| a.name == b.name);
            return hits;
        }
    }
    vec![]
}

const START_APPS_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

fn start_apps_cache() -> Option<PathBuf> {
    Some(aliases_path()?.with_file_name("apps-cache.txt"))
}

/// Parses `name<TAB>id` lines, one per app Windows lists in Start.
fn parse_start_apps(listing: &str) -> Vec<App> {
    listing
        .lines()
        .filter_map(|line| {
            let (name, id) = line.split_once('\t')?;
            let target = PathBuf::from(format!(r"shell:AppsFolder\{}", id.trim()));
            Some(App { name: name.trim().to_lowercase(), target })
        })
        .collect()
}

/// The saved Start app listing, if there is one from the last day.
fn cached_start_apps() -> Option<Vec<App>> {
    let cache = start_apps_cache()?;
    let age = fs::metadata(&cache).ok()?.modified().ok()?.elapsed().ok()?;
    if age > START_APPS_MAX_AGE {
        return None;
    }
    Some(parse_start_apps(&fs::read_to_string(cache).ok()?))
}

/// Asks Windows for every app it lists in Start. Unlike the shortcut scan this
/// includes Store apps, but it goes through PowerShell and takes about a second,
/// so the listing is saved for `cached_start_apps`.
fn fetch_start_apps() -> Vec<App> {
    let script = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
                  Get-StartApps | ForEach-Object { $_.Name + [char]9 + $_.AppID }";
    let Ok(out) = Command::new("powershell").args(["-NoProfile", "-Command", script]).output()
    else {
        return vec![];
    };
    let listing = String::from_utf8_lossy(&out.stdout);
    if let Some(cache) = start_apps_cache()
        && !listing.trim().is_empty()
    {
        fs::write(cache, listing.as_bytes()).ok();
    }
    parse_start_apps(&listing)
}

/// Adds the apps from `more` whose names aren't in `apps` yet.
fn add_new(apps: &mut Vec<App>, more: Vec<App>) {
    for app in more {
        if !apps.iter().any(|a| a.name == app.name) {
            apps.push(app);
        }
    }
}

/// Finds `name` in the PATH directories; a bare name gets `.exe` appended.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let mut file = PathBuf::from(name);
    if file.extension().is_none() {
        file.set_extension("exe");
    }
    env::split_paths(&env::var_os("PATH")?).map(|dir| dir.join(&file)).find(|p| p.is_file())
}

/// True if `path` is a readable PE file built for the console subsystem.
// ponytail: Store app aliases (wt, winget) are unreadable stubs, so they count as
// not-console and still launch; resolve the alias target if one ever misbehaves.
fn is_console_exe(path: &Path) -> bool {
    let mut head = vec![];
    let Ok(file) = fs::File::open(path) else { return false };
    file.take(4096).read_to_end(&mut head).ok();
    let le = |at: usize, n: usize| {
        head.get(at..at + n).map(|b| b.iter().rev().fold(0usize, |v, &x| v << 8 | x as usize))
    };
    // 0x3C holds the PE header offset; Subsystem sits 0x5C into it, 3 = console
    le(0x3C, 4).and_then(|pe| le(pe + 0x5C, 2)) == Some(3)
}


fn aliases_path() -> Option<PathBuf> {
    Some(PathBuf::from(env::var("APPDATA").ok()?).join(r"calamitus\aliases.txt"))
}

/// Loads user-defined `name=target` aliases, for apps the Start Menu scan can't find
/// (e.g. Store/UWP apps like WhatsApp, which use a `shell:AppsFolder\...` target instead of a .lnk).
fn load_aliases() -> Vec<(String, String)> {
    let Some(path) = aliases_path() else { return vec![] };
    let Ok(contents) = fs::read_to_string(&path) else {
        if path.exists() {
            return vec![];
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).ok();
        }
        fs::write(
            &path,
            "# map names to launch targets, one per line: name=target\n\
             # (or let launch write the line for you: launch --add <name> <path>)\n\
             # target can be a .lnk path, an .exe path, or shell:AppsFolder\\<id> for Store apps\n\
             # (find Store app ids with: powershell -Command \"Get-StartApps\")\n\
             # whatsapp=shell:AppsFolder\\5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App\n",
        ).ok();
        return vec![];
    };
    contents.lines().filter_map(parse_alias).collect()
}

/// Parses one `name=target` line; blank lines and `#` comments give `None`.
fn parse_alias(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (name, target) = line.split_once('=')?;
    Some((name.trim().to_lowercase(), target.trim().to_string()))
}

/// `contents` with `name` pointing at `target`, replacing any line that already defines `name`.
fn with_alias(contents: &str, name: &str, target: &str) -> String {
    let mut out: String = contents
        .lines()
        .filter(|line| parse_alias(line).is_none_or(|(n, _)| n != name))
        .map(|line| format!("{line}\n"))
        .collect();
    out.push_str(&format!("{name}={target}\n"));
    out
}

/// Splits `--add` arguments into the name and, if the last one looks like a
/// location (a path, `shell:...`, a URL), that location. Otherwise it is all name.
fn split_add_args(args: &[String]) -> (String, Option<String>) {
    let looks_like_target = |s: &str| s.contains(['\\', '/', ':']) || Path::new(s).exists();
    let (name, target) = match args.split_last() {
        Some((last, rest)) if looks_like_target(last) => (rest, Some(last.clone())),
        _ => (args, None),
    };
    (name.join(" ").trim().to_lowercase(), target)
}

/// Opens the Windows "choose a file" window; returns the picked path, or `None` if cancelled.
fn pick_file(name: &str) -> Option<String> {
    // links are not dereferenced, so picking a shortcut keeps the shortcut's own settings;
    // the topmost owner form stops the window opening behind the terminal
    let script = "Add-Type -AssemblyName System.Windows.Forms; \
                  $d = New-Object System.Windows.Forms.OpenFileDialog; \
                  $d.Title = 'Pick what to open for: launch ' + $env:LAUNCH_NAME; \
                  $d.Filter = 'Apps and shortcuts|*.exe;*.lnk;*.bat;*.cmd;*.url|All files|*.*'; \
                  $d.DereferenceLinks = $false; \
                  $owner = New-Object System.Windows.Forms.Form -Property @{ TopMost = $true }; \
                  if ($d.ShowDialog($owner) -eq 'OK') { \
                  [Console]::OutputEncoding = [Text.Encoding]::UTF8; $d.FileName }";
    let out = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", script])
        .env("LAUNCH_NAME", name)
        .output()
        .ok()?;
    let picked = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!picked.is_empty()).then_some(picked)
}

/// `launch --add [name...] [target]`: saves an alias so `launch <name>` opens `target`.
/// A missing name is asked for; a missing target is picked in a file window.
fn add_alias(args: &[String]) {
    let fail = |msg: &str| -> ! {
        eprintln!("{msg}");
        std::process::exit(1)
    };
    let (mut name, target) = split_add_args(args);
    if name.is_empty() {
        name = ask("name to launch it by: ").to_lowercase();
    }
    if name.is_empty() || name.contains('=') || name.starts_with('#') {
        fail("a name is needed, and it can't contain '=' or start with '#'");
    }
    let Some(target) = target.or_else(|| pick_file(&name)) else {
        fail("nothing picked, nothing added")
    };
    let target = if Path::new(&target).exists() {
        std::path::absolute(&target).map_or(target.clone(), |p| p.display().to_string())
    } else if target.starts_with("shell:") || target.contains("://") {
        target
    } else {
        fail(&format!("not found: {target}"))
    };

    load_aliases(); // creates the file with its header comments on first use
    let Some(path) = aliases_path() else { fail("APPDATA is not set") };
    let saved = fs::read_to_string(&path)
        .and_then(|contents| fs::write(&path, with_alias(&contents, &name, &target)));
    if let Err(e) = saved {
        fail(&format!("couldn't update {}: {e}", path.display()));
    }
    println!("added: {name} -> {target}");
}

/// Prompts the user to pick one of `candidates` by number. Returns the chosen app, or exits on bad input.
fn prompt_choice<'a>(header: &str, candidates: &[&'a App]) -> &'a App {
    eprintln!("{header}");
    for (i, c) in candidates.iter().enumerate() {
        eprintln!("  {}) {}", i + 1, c.name);
    }
    match ask(&format!("select [1-{}]: ", candidates.len())).parse::<usize>() {
        Ok(n) if n >= 1 && n <= candidates.len() => candidates[n - 1],
        _ => {
            eprintln!("invalid selection");
            std::process::exit(1);
        }
    }
}

/// Prints `prompt` and returns the line the user types, trimmed.
fn ask(prompt: &str) -> String {
    eprint!("{prompt}");
    io::stderr().flush().ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        std::process::exit(1);
    }
    input.trim().to_string()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            eprintln!("usage: launch <app name>");
            eprintln!("       launch --add [name] [path]   save an app launch can't find; leave out");
            eprintln!("                                    the path to pick the app in a window");
            eprintln!("       launch --edit                open your saved names in a text editor");
            std::process::exit(1);
        }
        Some("--add") => return add_alias(&args[1..]),
        Some("--edit") => {
            load_aliases(); // creates the file on first use
            if let Some(path) = aliases_path() {
                launch(&path);
            }
            return;
        }
        _ => {}
    }

    let query = args.join(" ").to_lowercase();

    if let Some((_, target)) = load_aliases().into_iter().find(|(name, _)| *name == query) {
        launch(&PathBuf::from(target));
        return;
    }

    // single-word queries naming a windowed program on PATH (notepad, calc, ...) get
    // launched directly, skipping Start Menu matching entirely. Console programs are
    // left alone: run bare they would fight the shell for this terminal, and some
    // (logoff, tsdiscon) act immediately when given no arguments.
    if let [arg] = args.as_slice()
        && let Some(exe) = find_on_path(arg)
        && !is_console_exe(&exe)
        && Command::new(&exe).spawn().is_ok()
    {
        return;
    }

    let mut apps = vec![];
    for dir in start_menu_dirs() {
        collect_shortcuts(&dir, &mut apps);
    }

    // Store apps have no shortcut to scan. When no shortcut's name contains the query,
    // bring in Windows' own app list before settling for a looser match.
    let query_compact = compact(&query);
    if !apps.iter().any(|a| compact(&a.name).contains(&query_compact)) {
        let cached = cached_start_apps();
        let was_cached = cached.is_some();
        add_new(&mut apps, cached.unwrap_or_else(fetch_start_apps));
        // a miss against a saved listing may just mean the app was installed since
        if was_cached && find_matches(&query, &apps).is_empty() {
            add_new(&mut apps, fetch_start_apps());
        }
    }
    if apps.is_empty() {
        eprintln!("no apps found");
        std::process::exit(1);
    }

    let matches = find_matches(&query, &apps);
    let chosen = match matches.as_slice() {
        [only] => only,
        [] => {
            // nothing matched: fall back to closest names by edit distance
            let mut by_distance: Vec<&App> = apps.iter().collect();
            by_distance.sort_by_key(|a| levenshtein(&a.name, &query));
            by_distance.truncate(5);
            prompt_choice(&format!("no app found matching '{query}', did you mean:"), &by_distance)
        }
        many => prompt_choice(&format!("multiple matches for '{query}':"), many),
    };
    launch(&chosen.target);
}

fn launch(target: &Path) {
    if let Err(e) = Command::new("explorer").arg(target).spawn() {
        eprintln!("failed to launch {}: {e}", target.display());
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(query: &str, apps: &[App]) -> Vec<String> {
        find_matches(query, apps).into_iter().map(|a| a.name.clone()).collect()
    }

    #[test]
    fn start_app_listing_is_parsed() {
        let apps = parse_start_apps("Minecraft Launcher\tPkg_x!Minecraft\r\nnot a listing line\n");
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "minecraft launcher");
        assert_eq!(apps[0].target, Path::new(r"shell:AppsFolder\Pkg_x!Minecraft"));
    }

    #[test]
    fn matching_uses_the_tightest_tier() {
        let apps: Vec<App> = [
            "visual studio code",
            "visual studio code",
            "code blocks",
            "voice recorder",
            "google chrome",
            "microsoft to do",
        ]
        .map(|name| App { name: name.into(), target: PathBuf::new() })
        .into();

        assert_eq!(found("google chrome", &apps), ["google chrome"]);
        // substring tier: both contain "code", shortest first, duplicate dropped
        assert_eq!(found("code", &apps), ["code blocks", "visual studio code"]);
        assert_eq!(found("todo", &apps), ["microsoft to do"]);
        // subsequence tier only kicks in when nothing contains the query
        assert_eq!(found("vs code", &apps), ["visual studio code"]);
        assert!(found("zzz", &apps).is_empty());
    }

    #[test]
    fn adding_an_alias_replaces_only_its_own_line() {
        let file = "# comment with name=target in it\ntodo=shell:x!App\nMinecraft = old.exe\n";
        assert_eq!(
            with_alias(file, "minecraft", "new.exe"),
            "# comment with name=target in it\ntodo=shell:x!App\nminecraft=new.exe\n"
        );
        assert_eq!(with_alias("", "a b", "x"), "a b=x\n");
        // what gets written must read back as the same alias
        assert_eq!(parse_alias("a b=x"), Some(("a b".into(), "x".into())));
    }

    #[test]
    fn add_arguments_split_into_name_and_location() {
        let split = |args: &[&str]| {
            split_add_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        };
        assert_eq!(split(&["Minecraft", "Launcher"]), ("minecraft launcher".into(), None));
        assert_eq!(split(&["mc", r"C:\Games\mc.exe"]), ("mc".into(), Some(r"C:\Games\mc.exe".into())));
        assert_eq!(split(&["wa", "shell:AppsFolder\\x!App"]).0, "wa");
        assert_eq!(split(&[]), (String::new(), None));
    }

    #[test]
    fn console_exes_are_detected() {
        // the test harness itself is a console program; explorer is not
        assert!(is_console_exe(&env::current_exe().unwrap()));
        assert!(!is_console_exe(&PathBuf::from(env::var("WINDIR").unwrap()).join("explorer.exe")));
        assert!(!is_console_exe(Path::new("no-such-file.exe")));
    }
}
