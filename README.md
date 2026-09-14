# launch

A tiny CLI to launch Windows apps by typing (part of) their name — searches
your Start Menu shortcuts and opens the best match.

```
launch notepad
launch chrome
```

If more than one shortcut matches, it lists them so you can be more specific.

## Install

Windows, no Rust required:

```powershell
irm https://raw.githubusercontent.com/aaditya19saini/calamitus-/master/install.ps1 | iex
```

This downloads the latest `launch.exe` release and adds it to your PATH.

## Build from source

```
cargo build --release
```
