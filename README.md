# calamitus

A tiny tool that lets you open apps by typing their name in a terminal, instead of clicking through the Start Menu.

```
launch notepad
```

That's it. That's the whole tool.

## Why would I want this?

If you're comfortable in a terminal, typing is usually faster than reaching for the mouse, opening the Start Menu, and clicking through a search result. `calamitus` gives you a shortcut command called `launch` that does the same thing the Start Menu search does, just from wherever your terminal already is.

## Installing it

You don't need to know Rust or programming to install this. Pick whichever option is easiest for you.

**Option A: one-line install script (works right now)**

```powershell
irm https://raw.githubusercontent.com/aaditya19saini/calamitus-/master/install.ps1 | iex
```

Copy that, paste it into PowerShell, hit Enter. It downloads the program and makes the `launch` command available everywhere.

If PowerShell won't let it run, close and reopen your terminal — that's usually all it takes.

**Option B: winget (pending approval, not live yet)**

`calamitus` has a package submitted to the winget repository, waiting on review. Once it's approved, anyone on Windows 10/11 will be able to run:

```powershell
winget install Calamitus.Launch
```

Until then, use Option A.

## How to use it

Just type `launch` followed by any part of an app's name:

```
launch chrome
launch spotify
launch vs code
```

You don't need the exact full name. If you type something that matches more than one app, it'll list the matches so you can be more specific.

## Frequently asked questions

**Does this slow down my computer?**
No. It only runs when you type `launch` — there's nothing sitting in the background.

**Is this safe?**
Yes. It only looks at the shortcuts already in your Start Menu and opens them the same way Windows Explorer would. It doesn't touch anything else on your system.

**What if it can't find my app?**
It searches your Start Menu shortcuts, so if an app doesn't show up when you search the Start Menu either, `launch` won't find it. Some apps (especially ones installed from a store rather than an installer) don't always add a shortcut there.

**Do I need to reinstall it every time I restart my PC?**
No. Once it's installed, it stays installed, just like any other program.

## Building it yourself

If you do know Rust and want to build from source instead:

```
cargo build --release
```

## License

MIT — see [LICENSE](LICENSE).
