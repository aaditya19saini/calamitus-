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

**Option A: winget**

```powershell
winget install Calamitus.Launch
```

Works on Windows 10/11 out of the box.

**Option B: one-line install script**

```powershell
irm https://raw.githubusercontent.com/aaditya19saini/calamitus-/master/install.ps1 | iex
```

Copy that, paste it into PowerShell, hit Enter. It downloads the program and makes the `launch` command available everywhere.

If PowerShell won't let it run, close and reopen your terminal — that's usually all it takes.

## How to use it

Just type `launch` followed by any part of an app's name:

```
launch chrome
launch spotify
launch vs code
```

You don't need the exact full name. If you type something that matches more than one app, it'll list the matches and ask you to pick one by number. If nothing matches, it'll suggest the closest names it can find.

## Apps it can't find on its own

`launch` finds anything Windows lists in your Start Menu, including apps installed from the Microsoft Store. An app that isn't listed there (a game launcher sitting in some folder, a portable app you unzipped) won't be found by itself.

To fix that, show `launch` where the app is, once, and give it whatever name you like:

```
launch --add minecraft
```

A normal "choose a file" window opens. Click your way to the app, press Open, and from then on `launch minecraft` opens it. You can also type just `launch --add` and it will ask you for the name first.

If you already know where the app is, you can skip the window by putting the location at the end:

```
launch --add minecraft "C:\Games\Minecraft\MinecraftLauncher.exe"
```

Written this way, the location can be a program, a shortcut, or a folder. A name you add always wins over anything else with the same name, and adding the same name again replaces the old location.

**To change or remove a name**, run `launch --edit`. It opens your list (`%APPDATA%\calamitus\aliases.txt`) in a text editor, one `name=location` per line.

## Frequently asked questions

**Does this slow down my computer?**
No. It only runs when you type `launch` — there's nothing sitting in the background.

**Is this safe?**
Yes. It only opens things you could already open yourself: the apps in your Start Menu, windowed programs on your PATH (like `notepad`), and any aliases you've added. It never runs command-line tools, and it doesn't touch anything else on your system.

**What if it can't find my app?**
It searches the same list of apps the Start Menu does, so if an app doesn't show up when you search the Start Menu either, `launch` won't find it. See [Apps it can't find on its own](#apps-it-cant-find-on-its-own) for how to add it yourself. An app you installed from the Store a moment ago can take one extra second to be found the first time.

**Do I need to reinstall it every time I restart my PC?**
No. Once it's installed, it stays installed, just like any other program.

## Building it yourself

If you do know Rust and want to build from source instead:

```
cargo build --release
```

## License

MIT — see [LICENSE](LICENSE).
