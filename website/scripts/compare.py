#!/usr/bin/env python3
"""Generates the comparison pages in public/compare/ (decision 033).

The facts live here, one dict per tool, each checked against the sources it
lists on the date in DATE; the pages are its output, committed as deployable
HTML. Run `task web:compare` (this, then the partials stamp and Prettier)
after changing a fact, and `task web:render -- pages` if a headline, lead or
table row on a card changed. Rules for the copy and the bolding are in
docs/DESIGN.md (Website, Taste).

Usage: python3 scripts/compare.py public
"""
import html, json, os, re, sys
OUT = sys.argv[1]
SITE = "https://smolcast.evanlee.engineer"
DATE = "2026-10-23"; DATE_TEXT = "23 Oct 2026"

KEYS={"Ctrl":"Control","Alt":"Alt","Space":" ","Win":"Meta",".":".","⌥":"Alt"}
def k(*keys): return "".join(f'<kbd class="cap" data-key="{KEYS[x]}">{x}</kbd>' for x in keys)
Y,P,N = "yes","part","no"
HOTKEY = k("Ctrl","Alt","Space")  # smolcast's default hotkey

# Each cell: (status or None, label/text, note or None, win)
US = {
 "price": (None,"Free",None,False),
 "license": (None,"Open source (MIT)",None,False),
 "os": (None,"Windows 11",None,False),
 "account": (None,"None",None,False),
 "hotkey": (None,HOTKEY,None,False),
 "clip": (Y,"Built in","Text and images. Encrypted on disk",False),
 "snip": (Y,"Built in","Find by name or keyword. No auto-expand",False),
 "emoji": (Y,"Built in","Every emoji, in color",False),
 "files": (Y,"Built in","Through Windows Search",False),
 "calc": (Y,"Built in","Units too. Enter pastes",False),
 "links": (Y,"Built in","Keywords with {query}",False),
 "ext": (N,"No","Scripts and quicklinks instead",False),
 "ai": (N,"No",None,False),
 "win": (N,"No",None,False),
 "size": (None,"7 MB, one .exe","No installer",False),
 "idle": (None,"0% CPU while hidden",None,False),
 "privacy": (None,"No telemetry",'<a href="/privacy">One daily update check</a>, can be turned off',False),
}
ROWS = [("The basics",[("price","Price"),("license","Source"),("os","Runs on"),("account","Account"),("hotkey","Default hotkey")]),
 ("Features",[("clip","Clipboard history"),("snip","Snippets"),("emoji","Emoji picker"),("files","File search"),("calc","Calculator"),("links","Quicklinks"),("ext","Extensions"),("ai","AI"),("win","Window management")]),
 ("Under the hood",[("size","Download"),("idle","Idle"),("privacy","Privacy")])]

def win(cell): return (cell[0],cell[1],cell[2],True)
def marked(keys):
  cells = dict(US)
  for key in keys: cells[key]=win(cells[key])
  return cells
def nl(note=None): return (N,"Not listed",note,False)

PAGES = [
 dict(slug="raycast", visit="https://www.raycast.com/windows?ref=smolcast", name="Raycast", full="Raycast for Windows", url="https://www.raycast.com/windows",
  checked='<a href="https://www.raycast.com/windows">raycast.com/windows</a> (v2.9)',
  sources=[("Raycast for Windows 2.9", "https://www.raycast.com/windows", "raycast.com/windows"), ("Raycast pricing", "https://www.raycast.com/pricing", "raycast.com/pricing"), ("Raycast privacy policy (usage data)", "https://www.raycast.com/privacy", "raycast.com/privacy")],
  title="smolcast vs Raycast for Windows: a free Raycast alternative",
  desc="Raycast for Windows has extensions, AI and window management, with a paid Pro plan. smolcast does the everyday stuff in a free, open-source 7 MB app with no account.",
  lead="Raycast brings an extension store, AI and Mac sync. smolcast skips all three and fits in 7 MB with no account.",
  us_tags=["Free","Open source","7 MB","No account"], them_tags=["Free + Pro $10/mo","Closed source","Extensions","AI"],
  us_best="the essentials, small and private", them_best="extensions, AI and a Mac + PC setup", them_by="Raycast",
  them=dict(price=(None,"Free; Pro $10/mo",None,False), license=(None,"Closed source","Extensions are open",False),
   os=(None,"Windows 10 and 11, plus macOS",None,True), account=(None,"Needed for Pro, AI and sync",None,False),
   hotkey=(None,"Configurable",None,False),
   clip=(Y,"Built in",None,False), snip=(Y,"Built in","Expands by keyword",True), emoji=(Y,"Built in",None,False), files=(Y,"Built in",None,False),
   calc=(Y,"Built in",None,False), links=(Y,"Built in",None,False), ext=(Y,"Raycast Store","Shared with the Mac app",True), ai=(P,"Pro","Quick AI and AI Chat",True),
   win=(Y,"Built in",None,True), size=(None,"Microsoft Store or WinGet","Size not published",False), idle=(None,"Not published",None,False),
   privacy=(None,"Collects usage data for analytics","Per its privacy policy",False)),
  us_wins=["account","license","privacy"],
  them_if=["You want extensions (Linear, GitHub, Notion, Spotify…)","You want AI right in the launcher","You need window management","You also use Raycast on a Mac","You're on Windows 10"],
  us_if=["You just want the essentials, fast","You'd rather not make an account","You like a small app you can read the code of","You want zero AI and zero telemetry"],
  maps=[("Quicklinks","<code>[quicklinks.gh]</code> in <code>config.toml</code>, same <code>{query}</code> idea"),
   ("Snippets","<code>[snippets.sig]</code> with a keyword. Found in the palette, not expanded as you type"),
   ("Script commands","<em>Track Script File</em>: any <code>.ps1</code>, <code>.cmd</code>, <code>.py</code> or program, with its own hotkey"),
   ("Calculator","Same idea, but Enter pastes the answer instead of copying it")]),
 dict(slug="powertoys-command-palette", visit="https://learn.microsoft.com/windows/powertoys/install?ref=smolcast", name="Command Palette", full="PowerToys Command Palette", url="https://learn.microsoft.com/windows/powertoys/command-palette/overview",
  checked='<a href="https://learn.microsoft.com/windows/powertoys/command-palette/overview">Microsoft\'s docs</a> (PowerToys 0.102)',
  sources=[("PowerToys Command Palette overview, updated October 2026", "https://learn.microsoft.com/windows/powertoys/command-palette/overview", "learn.microsoft.com"), ("PowerToys 0.102.2933.0 release (installer size)", "https://github.com/microsoft/PowerToys/releases/tag/v0.102.2933.0", "github.com/microsoft/PowerToys")],
  title="smolcast vs PowerToys: a PowerToys Run alternative",
  desc="PowerToys Command Palette is Microsoft's extensible launcher, great for Windows admin tasks. smolcast is a single 7 MB app with clipboard history, emoji and snippets built in.",
  lead="Command Palette, the successor to PowerToys Run, reaches deep into Windows: WinGet, services, the registry. smolcast adds clipboard history, emoji and snippets instead.",
  us_tags=["Free","Open source","7 MB","Standalone"], them_tags=["Free","Open source","Part of PowerToys","Extensions"],
  us_best="clipboard, emoji and snippets in one tiny app", them_best="Windows admin and extensions", them_by="Microsoft",
  them=dict(price=(None,"Free",None,False), license=(None,"Open source (MIT)",None,False),
   os=(None,"Windows 10 and 11",None,True), account=(None,"None",None,False), hotkey=(None,k("Win","Alt","Space"),None,False),
   clip=(Y,"Built in","Recent entries",False), snip=(N,"No","Bookmarks with {query} instead",False), emoji=(N,"No","Windows has " + k("Win",".") ,False),
   files=(Y,"Built in",None,False), calc=(Y,"Built in","Math and functions",False), links=(Y,"Bookmarks",None,False),
   ext=(Y,"Extension Gallery",None,True), ai=(N,"No",None,False), win=(P,"Window switching",None,True),
   size=(None,"309 MB installer","The whole PowerToys suite",False), idle=(None,"Not published",None,False),
   privacy=(None,"Diagnostic data","Can be turned off",False)),
  us_wins=["snip","emoji","size","privacy"],
  them_if=["You already run PowerToys","You want WinGet, services, the registry and Settings in the launcher","You want extensions or the Dock","You're on Windows 10"],
  us_if=["You want clipboard, emoji and snippets without add-ons","You want unit conversions like 40cm to in","You want one small app, not a suite","You want no telemetry"],
  maps=[("PowerToys Run","Command Palette replaced it, so this page covers both; smolcast\'s " + HOTKEY + " won\'t clash with Run\'s " + k("Alt","Space")),
   ("Bookmarks","Quicklinks: <code>[quicklinks.&lt;keyword&gt;]</code> with a URL, file or folder and <code>{query}</code>"),
   ("Hotkey",HOTKEY + " by default, so you can run both while you try it"),
   ("Shell commands","Tracked script files, each with an optional hotkey")]),
 dict(slug="flow-launcher", visit="https://www.flowlauncher.com/?ref=smolcast", name="Flow Launcher", full="Flow Launcher", url="https://www.flowlauncher.com/",
  checked='<a href="https://github.com/Flow-Launcher/Flow.Launcher">its README and releases</a> (v2.1.4)',
  sources=[("Flow Launcher README", "https://github.com/Flow-Launcher/Flow.Launcher", "github.com/Flow-Launcher"), ("Flow Launcher 2.1.4 release (installer size)", "https://github.com/Flow-Launcher/Flow.Launcher/releases/tag/v2.1.4", "github.com/Flow-Launcher"), ("Flow Launcher attribution (its Wox roots)", "https://github.com/Flow-Launcher/Flow.Launcher/blob/dev/attribution.md", "github.com/Flow-Launcher")],
  title="smolcast vs Flow Launcher: a Flow Launcher alternative",
  desc="Flow Launcher is a plugin-driven open-source launcher for Windows with great file search. smolcast builds in clipboard history, emoji and snippets, in a 7 MB app.",
  lead="Flow Launcher grows with plugins and searches inside files through Everything. smolcast has no plugins and builds clipboard history, emoji and snippets in.",
  us_tags=["Free","Open source","7 MB","Batteries included"], them_tags=["Free","Open source","Plugins","Everything search"],
  us_best="built-in clipboard, emoji and snippets", them_best="plugins and deep file search", them_by="Open-source community",
  them=dict(price=(None,"Free",None,False), license=(None,"Open source (MIT)",None,False),
   os=(None,"Windows 10 and 11",None,True), account=(None,"None",None,False), hotkey=(None,k("Alt","Space"),None,False),
   clip=(P,"Plugin",None,False), snip=(N,"No",None,False), emoji=(N,"No",None,False),
   files=(Y,"Built in","Names and contents, via Everything",True), calc=(Y,"Built in","Copies the result",False), links=(Y,"Web searches",None,False),
   ext=(Y,"Plugin store","C#, Python, JS and more",True), ai=(N,"No",None,False), win=(N,"No",None,False),
   size=(None,"97 MB installer",None,False), idle=(None,"Not published",None,False), privacy=(None,"See the project",None,False)),
  us_wins=["clip","snip","emoji","size"],
  them_if=["You want plugins, or to write your own","You use Everything and search inside files","You want themes and browser bookmarks","You're on Windows 10"],
  us_if=["You want clipboard, emoji and snippets out of the box","You want unit conversions that paste on Enter","You want one small .exe and nothing else","You'd rather not manage plugins"],
  maps=[("Web searches","Quicklinks with a <code>{query}</code> URL, and every search ends with your engine"),
   ("Shell commands","Tracked script files, each with an optional hotkey"),
   ("Hotkey",HOTKEY + " by default, so it won't fight Flow's " + k("Alt","Space"))]),
 dict(slug="tinycast", visit="https://tinycast.dev/?ref=smolcast", name="Tinycast", full="Tinycast", url="https://tinycast.dev",
  checked='<a href="https://tinycast.dev">tinycast.dev</a> (v0.12.1)',
  sources=[("Tinycast 0.12.1", "https://tinycast.dev", "tinycast.dev")],
  title="smolcast vs Tinycast: a Tinycast alternative for Windows",
  desc="Tinycast is a free, open-source Raycast-style launcher for macOS only. smolcast is the same idea for Windows 11: free, open source and 7 MB, with fewer features.",
  lead="Tinycast is Mac only. smolcast is the same idea for Windows 11, its own project, and it does less.",
  us_tags=["Windows 11","Open source","7 MB","No account"], them_tags=["macOS","Open source","No account","Optional AI"],
  us_best="the Tinycast feel on Windows", them_best="everything, on a Mac", them_by="Open-source project",
  them=dict(price=(None,"Free; tips welcome",None,False), license=(None,"Open source (AGPL-3.0)",None,False),
   os=(None,"macOS 26+",None,False), account=(None,"None",None,False), hotkey=(None,"Configurable",None,False),
   clip=(Y,"Built in","Also files and colors",True), snip=(Y,"Built in","Markdown and placeholders",True), emoji=(Y,"Built in","Plus symbols",False),
   files=(Y,"Built in",None,False), calc=(Y,"Built in","Plus currency and time zones",True), links=(Y,"Built in",None,False),
   ext=(P,"Raycast extensions","Off by default",True), ai=(P,"Optional","Off by default",True), win=(Y,"Built in","35 commands, saved layouts",True),
   size=(None,"Mac app",None,False), idle=(None,"Under 100 MB of memory",None,False), privacy=(None,"No telemetry",None,False)),
  us_wins=[],
  them_if=["You're on a Mac (smolcast isn't)","You want window management, notes or calendar","You want optional AI or Raycast extensions"],
  us_if=["You're on Windows 11","You use Tinycast on a Mac and want the same feel on your PC","You prefer the MIT license","You want something tiny with fewer moving parts"],
  maps=[("Quicklinks","<code>[quicklinks.&lt;keyword&gt;]</code> with <code>{query}</code>, the same idea"),
   ("Snippets","<code>[snippets.&lt;id&gt;]</code> with a keyword (no placeholders yet)"),
   ("Hotkey",HOTKEY + ", the Windows reach of " + k("⌥","Space"))]),
 dict(slug="fluent-search", visit="https://www.fluentsearch.net/?ref=smolcast", name="Fluent Search", full="Fluent Search", url="https://www.fluentsearch.net/",
  checked='<a href="https://www.fluentsearch.net/">fluentsearch.net</a> and its docs',
  sources=[("Fluent Search site", "https://www.fluentsearch.net/", "fluentsearch.net"), ("Fluent Search docs", "https://www.fluentsearch.net/docs", "fluentsearch.net/docs"), ("Fluent Search repository", "https://github.com/adirh3/Fluent-Search", "github.com/adirh3/Fluent-Search")],
  title="smolcast vs Fluent Search: a Fluent Search alternative",
  desc="Fluent Search searches everything on your PC, from browser tabs to text on screen, with its own file indexer and C# plugins. smolcast is a small, open-source launcher with clipboard history, emoji and snippets built in.",
  lead="Fluent Search searches tabs, windows, files and even text on screen. smolcast searches less and pastes more: clipboard history, emoji and snippets.",
  us_tags=["Free","Open source","7 MB","No account"], them_tags=["Free","Own file indexer","Screen search","Plugins"],
  us_best="clipboard, emoji and snippets in a tiny app", them_best="searching everything, even text on screen", them_by="Independent project",
  them=dict(price=(None,"Free; donations welcome",None,False), license=(None,"License not published",None,False),
   os=(None,"Windows 10 and 11, x64 and ARM64",None,False), account=(None,"Not published",None,False), hotkey=(None,"Not published",None,False),
   clip=nl(), snip=nl(), emoji=nl(), files=(Y,"Built in","Its own native file indexer",False),
   calc=(Y,"Built in",None,False), links=(Y,"Web search",None,False), ext=(Y,"C# plugins",None,False),
   ai=(P,"AI Search",None,False), win=(P,"Window search",None,False),
   size=(None,"Installer, APPX, portable or WinGet","Size not published",False), idle=(None,"Not published",None,False), privacy=(None,"See its policy",None,False)),
  us_wins=["license"],
  them_if=["You want one search for apps, files, tabs and windows","You want to search text on screen with OCR","You want its own fast file index","You want C# plugins"],
  us_if=["You want clipboard history, emoji and snippets built in","You want unit conversions that paste on Enter","You want an MIT-licensed app you can read","You want one small .exe"],
  maps=[("Web searches","Quicklinks with a <code>{query}</code> URL, and every search ends with your engine"),
   ("File search","Search Files uses the Windows Search index, no index of its own"),
   ("Commands","Tracked script files, each with an optional hotkey")]),
 dict(slug="listary", visit="https://www.listary.com/?ref=smolcast", name="Listary", full="Listary", url="https://www.listary.com/",
  checked='<a href="https://www.listary.com/">listary.com</a>, its Pro page and its help',
  sources=[("Listary site", "https://www.listary.com/", "listary.com"), ("Listary Pro", "https://www.listary.com/pro", "listary.com/pro"), ("Listary hotkeys", "https://help.listary.com/hotkeys", "help.listary.com")],
  title="smolcast vs Listary: a Listary alternative for launching",
  desc="Listary is a file search tool for Windows that also jumps Open and Save dialogs to your folders, free with a $19.95 Pro. smolcast is a launcher with clipboard history, emoji and snippets built in.",
  lead="Listary finds files and points Open and Save dialogs straight at them. smolcast launches apps and pastes from clipboard history, emoji and snippets.",
  us_tags=["Free","Open source","7 MB","Launcher first"], them_tags=["Free + Pro $19.95","Files first","Quick Switch","Double Ctrl"],
  us_best="a launcher with clipboard, emoji and snippets", them_best="finding files and taming Open and Save dialogs", them_by="Little Carved",
  them=dict(price=(None,"Free; Pro $19.95 once",None,False), license=(None,"Source not published",None,False),
   os=(None,"Windows",None,False), account=(None,"Not published",None,False), hotkey=(None,"Double-tap Ctrl",None,False),
   clip=nl(), snip=nl(), emoji=nl(), files=(Y,"Built in","Its core, plus Quick Switch for dialogs",False),
   calc=nl(), links=(Y,"Web search",None,False), ext=nl(), ai=nl(), win=nl(),
   size=(None,"Installer","Size not published",False), idle=(None,"Not published",None,False), privacy=(None,"See its policy",None,False)),
  us_wins=["license"],
  them_if=["You mostly search for files","You want Open and Save dialogs to jump to your folders","You like double-tapping Ctrl","You need network-drive indexing (Pro)"],
  us_if=["You want a launcher with clipboard, emoji and snippets","You want a calculator with unit conversions","You want everything free and open source","You want one small .exe"],
  maps=[("File search","Search Files: type, then Enter opens and Ctrl+Enter shows it in Explorer"),
   ("Web search","Quicklinks with a <code>{query}</code> URL, and a web search row after every query"),
   ("Hotkey",HOTKEY + " by default, so it won't clash with double Ctrl")]),
 dict(slug="wox", visit="https://www.woxlauncher.com/?ref=smolcast", name="Wox", full="Wox", url="https://www.woxlauncher.com/",
  checked='<a href="https://www.woxlauncher.com/">woxlauncher.com</a> and <a href="https://github.com/Wox-launcher/Wox">its repository</a> (v2.4.7)',
  sources=[("Wox site", "https://www.woxlauncher.com/", "woxlauncher.com"), ("Wox 2.4.7 repository", "https://github.com/Wox-launcher/Wox", "github.com/Wox-launcher/Wox"), ("Flow Launcher attribution (its Wox roots)", "https://github.com/Flow-Launcher/Flow.Launcher/blob/dev/attribution.md", "github.com/Flow-Launcher")],
  title="smolcast vs Wox: a smaller Wox alternative without AI",
  desc="Wox is an open-source, cross-platform launcher with about 30 built-in plugins, AI chat and a plugin store. smolcast is a smaller, Windows-only app with the everyday features and no AI.",
  lead="Wox runs everywhere and does a lot, AI included. smolcast is Windows only, smaller, and stays out of AI.",
  us_tags=["Windows 11","Open source","7 MB","No AI"], them_tags=["Windows, Mac, Linux","Open source","Plugin store","AI"],
  us_best="a small, quiet Windows launcher", them_best="one launcher on every OS, with AI", them_by="Open-source project",
  them=dict(price=(None,"Free",None,False), license=(None,"Open source (GPL-3.0)",None,False),
   os=(None,"Windows, macOS and Linux",None,False), account=(None,"None; optional Cloud Sync",None,False), hotkey=(None,"Not published",None,False),
   clip=(Y,"Built in",None,False), snip=nl(), emoji=(Y,"Built in",None,False), files=(Y,"Built in",None,False),
   calc=(Y,"Built in","Plus a converter",False), links=(Y,"Web search",None,False), ext=(Y,"Plugin store","Node.js, Python or scripts",False),
   ai=(Y,"AI Chat",None,False), win=(P,"Window layouts",None,False),
   size=(None,"WinGet, Scoop or Chocolatey","Size not published",False), idle=(None,"About 150 MB of memory",None,False), privacy=(None,"See the project",None,False)),
  us_wins=[],
  them_if=["You want the same launcher on Windows, Mac and Linux","You want AI chat and AI commands","You want notes, timers, screenshots and more","You want a plugin store"],
  us_if=["You're on Windows 11 and want something small","You want no AI at all","You prefer the MIT license","You want unit conversions that paste on Enter"],
  maps=[("Web searches","Quicklinks with a <code>{query}</code> URL"),
   ("Shell commands","Tracked script files, each with an optional hotkey"),
   ("Clipboard","Clipboard History, or Windows' own history when it's on")]),
 dict(slug="alfred", visit="https://www.alfredapp.com/?ref=smolcast", name="Alfred", full="Alfred", url="https://www.alfredapp.com/",
  checked='<a href="https://www.alfredapp.com/">alfredapp.com</a>, its Powerpack page and help (v5.8.1)',
  sources=[("Alfred 5.8.1", "https://www.alfredapp.com/", "alfredapp.com"), ("Alfred Powerpack", "https://www.alfredapp.com/powerpack/", "alfredapp.com/powerpack"), ("Clipboard History help", "https://www.alfredapp.com/help/features/clipboard/", "alfredapp.com/help"), ("Snippets help", "https://www.alfredapp.com/help/features/snippets/", "alfredapp.com/help")],
  title="smolcast vs Alfred: an Alfred alternative for Windows",
  desc="Alfred is a Mac-only launcher with a free core and a paid Powerpack for clipboard history, snippets and workflows. smolcast is a free, open-source launcher for Windows 11 with those built in.",
  lead="Alfred is Mac only. On Windows, smolcast gives you a launcher with clipboard history and snippets built in, free.",
  us_tags=["Windows 11","Free","Open source","7 MB"], them_tags=["macOS","Free + Powerpack £34","Workflows","Snippet expansion"],
  us_best="an Alfred-style launcher on Windows", them_best="workflows and power features on a Mac", them_by="Running with Crayons",
  them=dict(price=(None,"Free; Powerpack £34",None,False), license=(None,"Source not published",None,False),
   os=(None,"macOS 10.14 or later",None,False), account=(None,"Not published",None,False), hotkey=(None,"Configurable",None,False),
   clip=(P,"Powerpack",None,False), snip=(P,"Powerpack","With text expansion",False), emoji=nl(), files=(Y,"Built in",None,False),
   calc=(Y,"Built in",None,False), links=(Y,"Web search",None,False), ext=(P,"Workflows","Powerpack",False),
   ai=nl(), win=nl(),
   size=(None,"Mac app",None,False), idle=(None,"Not published",None,False), privacy=(None,"See its policy",None,False)),
  us_wins=["clip","snip","license"],
  them_if=["You're on a Mac (smolcast isn't)","You want workflows","You want snippets that expand as you type"],
  us_if=["You're on Windows 11","You want clipboard history and snippets free","You want an open-source launcher","You want something tiny"],
  maps=[("Web searches","Quicklinks with <code>{query}</code>, typed after a keyword like Alfred's"),
   ("Snippets","<code>[snippets.&lt;id&gt;]</code> with a keyword; found in the palette, not expanded as you type"),
   ("Hotkey",HOTKEY + ", the Windows reach of " + k("⌥","Space"))]),
 dict(slug="keypirinha", visit="https://keypirinha.com/?ref=smolcast", name="Keypirinha", full="Keypirinha", url="https://keypirinha.com/",
  checked='<a href="https://keypirinha.com/">keypirinha.com</a> (v2.26, released November 2020)',
  sources=[("Keypirinha 2.26", "https://keypirinha.com/", "keypirinha.com"), ("Keypirinha releases", "https://github.com/Keypirinha/Keypirinha/releases", "github.com/Keypirinha")],
  title="smolcast vs Keypirinha: a maintained Keypirinha alternative",
  desc="Keypirinha is a fast, portable freeware launcher for Windows with Python plugins; its last release was in November 2020. smolcast is actively developed and builds in clipboard history, emoji and snippets.",
  lead="Keypirinha is fast, portable and scriptable in Python, but its last release was in 2020. smolcast is new and still shipping.",
  us_tags=["Free","Open source","7 MB","Actively developed"], them_tags=["Freeware","Portable","Python plugins","Last release 2020"],
  us_best="a launcher that still ships updates", them_best="a portable, scriptable launcher", them_by="Independent project",
  them=dict(price=(None,"Freeware",None,False), license=(None,"Freeware; source not published",None,False),
   os=(None,"Windows",None,False), account=(None,"None",None,False), hotkey=(None,"Not published",None,False),
   clip=nl(), snip=nl(), emoji=nl(), files=(Y,"Built in",None,False),
   calc=(Y,"Built in",None,False), links=(Y,"Web search",None,False), ext=(Y,"Python plugins",None,False),
   ai=nl(), win=(P,"Switch to a running app",None,False),
   size=(None,"Installer or portable","Size not published",False), idle=(None,"Not published",None,False), privacy=(None,"See its site",None,False)),
  us_wins=["license"],
  them_if=["You want a portable launcher","You want to write plugins in Python","You want to switch to running apps by name"],
  us_if=["You want a launcher that's still getting updates","You want clipboard, emoji and snippets built in","You want open source","You want unit conversions"],
  maps=[("Web searches","Quicklinks with a <code>{query}</code> URL"),
   ("Python plugins","Tracked script files: a <code>.py</code> runs with its arguments and an optional hotkey"),
   ("Portable","smolcast is one .exe in any folder you can write to; settings live in <code>%APPDATA%</code>")]),
 dict(slug="ueli", visit="https://ueli.app/?ref=smolcast", name="Ueli", full="Ueli", url="https://ueli.app/",
  checked='<a href="https://github.com/oliverschwendener/ueli">its repository and releases</a> (v9.29.1)',
  sources=[("Ueli repository", "https://github.com/oliverschwendener/ueli", "github.com/oliverschwendener/ueli"), ("Ueli 9.29.1 release (installer size)", "https://github.com/oliverschwendener/ueli/releases/tag/v9.29.1", "github.com/oliverschwendener/ueli")],
  title="smolcast vs Ueli: a native Ueli alternative",
  desc="Ueli is an open-source, cross-platform Electron launcher with toggleable extensions; its Windows installer is 177 MB. smolcast is a native 7 MB Windows app with clipboard history, emoji and snippets built in.",
  lead="Ueli is an open-source Electron launcher for every OS. smolcast is native, Windows only, and about 25 times smaller.",
  us_tags=["Native","Open source","7 MB","Windows 11"], them_tags=["Electron","Open source","Windows, Mac, Linux","Extensions"],
  us_best="a native, tiny Windows launcher", them_best="one open-source launcher on every OS", them_by="Open-source project",
  them=dict(price=(None,"Free",None,False), license=(None,"Open source (MIT)",None,False),
   os=(None,"Windows, macOS and Linux",None,False), account=(None,"None",None,False), hotkey=(None,k("Alt","Space"),None,False),
   clip=nl(), snip=nl(), emoji=nl(), files=(Y,"Built in",None,False),
   calc=(Y,"Built in","Plus currency conversion",False), links=(Y,"Web search",None,False), ext=(Y,"Extensions","Toggle on and off",False),
   ai=nl(), win=nl(),
   size=(None,"177 MB installer","Unsigned",False), idle=(None,"Not published",None,False), privacy=(None,"See the project",None,False)),
  us_wins=["size"],
  them_if=["You want the same launcher on Windows, Mac and Linux","You want currency conversion","You like toggling extensions on and off"],
  us_if=["You want a native app, not Electron","You want clipboard, emoji and snippets built in","You want a 7 MB download","You're on Windows 11"],
  maps=[("Web search","Quicklinks with a <code>{query}</code> URL, and a web row after every search"),
   ("Hotkey",HOTKEY + " by default, so it won't fight Ueli's " + k("Alt","Space")),
   ("Calculator","Math and unit conversions, offline; Enter pastes the answer")]),
]

# One sentence per page pointing at the comparisons its reader is most
# likely weighing next, with descriptive link text (DESIGN.md, discovery).
def vs(slug, name): return f'<a href="/compare/{slug}">smolcast vs {name}</a>'
RELATED = {
  "raycast": f"On a Mac too? Tinycast is the free, open-source take on Raycast, and Alfred is the long-standing favorite: {vs('tinycast', 'Tinycast')}, {vs('alfred', 'Alfred')}.",
  "powertoys-command-palette": f"For plugins without the whole suite there is {vs('flow-launcher', 'Flow Launcher')}, and for one search over everything, even text on screen, {vs('fluent-search', 'Fluent Search')}.",
  "flow-launcher": f"Flow Launcher began as a fork of Wox, which is still going: {vs('wox', 'Wox')}. Microsoft's own launcher is next door: {vs('powertoys-command-palette', 'PowerToys Command Palette')}.",
  "tinycast": f"Tinycast borrows from Raycast, which now runs on Windows too: {vs('raycast', 'Raycast for Windows')}. On the Mac, Alfred is the long-standing favorite: {vs('alfred', 'Alfred')}.",
  "fluent-search": f"For files first, see {vs('listary', 'Listary')}; for Everything's index in a launcher with plugins, {vs('flow-launcher', 'Flow Launcher')}.",
  "listary": f"For one search over files, tabs and windows, see {vs('fluent-search', 'Fluent Search')}; for Everything's index in a launcher, {vs('flow-launcher', 'Flow Launcher')}.",
  "wox": f"Flow Launcher began as a fork of Wox: {vs('flow-launcher', 'Flow Launcher')}. Another open-source launcher for every OS: {vs('ueli', 'Ueli')}.",
  "alfred": f"The free, open-source Mac cousin is Tinycast, and Raycast now has a Windows app: {vs('tinycast', 'Tinycast')}, {vs('raycast', 'Raycast for Windows')}.",
  "keypirinha": f"Two plugin launchers that still ship updates: {vs('flow-launcher', 'Flow Launcher')} and {vs('wox', 'Wox')}.",
  "ueli": f"Another open-source launcher for every OS: {vs('wox', 'Wox')}. Raycast now runs on Windows and Mac: {vs('raycast', 'Raycast for Windows')}.",
}

def esc_attr(s): return html.escape(s, quote=True)
ICON='<img src="/favicon.svg" alt="" width="24" height="24" />'

# Systems each tool runs on, for the "Runs on" row: app.js marks the cells
# that fit the visitor's OS.
OS={"smolcast":"win11","Raycast for Windows":"win10 win11 mac","PowerToys Command Palette":"win10 win11","Flow Launcher":"win10 win11","Tinycast":"mac",
  "Fluent Search":"win10 win11","Listary":"win10 win11","Wox":"win10 win11 mac","Alfred":"mac","Keypirinha":"win10 win11","Ueli":"win10 win11 mac"}

def cell(c, ours=False, os=None):
  st,label,note,w = c
  w = w and ours  # only smolcast's wins are bold
  if os: return f'<td data-os="{os}">{label}</td>'
  glyph = '<svg aria-hidden="true"><use href="/icons.svg#check" /></svg>' if st == Y else ""
  inner = f'<span class="status {st}">{glyph}{label}</span>' if st else label
  n = f'<span class="note">{note}</span>' if note else ""
  if w: return f'<td class="win"><strong>{inner}</strong>{n}<span class="sr-only"> (the better pick)</span></td>'
  return f"<td>{inner}{n}</td>"

def table(cols, rows=ROWS, wide=False):
  us = ' class="us"'
  head = '<tr><td></td>' + "".join(f'<th scope="col"{us if i == 0 else ""}>{n}</th>' for i,(n,_) in enumerate(cols)) + '</tr>'
  out=[f'        <table class="compare{" wide" if wide else ""}">', f'          <thead>{head}</thead>','          <tbody>']
  for group, rs in rows:
    out.append(f'            <tr class="group"><th scope="rowgroup" colspan="{len(cols)+1}">{group}</th></tr>')
    for key,label in rs:
      out.append(f'            <tr><th scope="row">{label}</th>' + "".join(cell(d[key], i == 0, OS[n] if key == "os" else None) for i,(n,d) in enumerate(cols)) + '</tr>')
  out += ['          </tbody>','        </table>']
  return "\n".join(out)

def tags(ts): return '<ul class="tags">' + "".join(f'<li class="tag">{t}</li>' for t in ts) + '</ul>'

def rows_html(current):
  items = [("/compare/", "Every alternative, side by side", "Alternatives")] if current != "hub" else []
  for p in PAGES:
    if p["slug"] != current: items.append((f"/compare/{p['slug']}", f"smolcast vs {p['full']}", "Alternative"))
  items.append(("/privacy", "What smolcast keeps on your PC", "Privacy"))
  out = ['        <ul class="rows">']
  for href, name, trail in items:
    out.append(f'          <li><a href="{href}"><svg aria-hidden="true"><use href="/icons.svg#doc" /></svg><span class="name">{name}</span><span class="trail">{trail}</span></a></li>')
  out.append('        </ul>')
  return "\n".join(out)

CTA = '''      <section class="cta" aria-labelledby="get-title">
        <img src="/favicon.svg" alt="" width="56" height="56" />
        <div>
          <h2 id="get-title">Try smolcast</h2>
          <ul class="tags"><li class="tag">Windows 11, x64</li><li class="tag">7 MB</li><li class="tag">Free and open source</li></ul>
        </div>
        <a class="button" href="https://github.com/Archetypically/smolcast/releases/latest"><svg aria-hidden="true"><use href="/icons.svg#download" /></svg>Download</a>
      </section>'''

def head(title, desc, path, ld, ogtype="article", alt=""):
  img = f"{SITE}/og/{path.strip('/').replace('/', '-')}.png"
  url = SITE + path
  return f'''<!doctype html>
<html lang="en">
  <head>
    <!-- partial:head -->
    <!-- /partial:head -->
    <title>{title} · smolcast</title>
    <meta name="description" content="{esc_attr(desc)}" />
    <link rel="canonical" href="{url}" />
    <meta name="robots" content="index, follow, max-snippet:-1" />
    <meta property="og:type" content="{ogtype}" />
    <meta property="og:site_name" content="smolcast" />
    <meta property="og:locale" content="en_US" />
    <meta property="og:url" content="{url}" />
    <meta property="og:title" content="{esc_attr(title)}" />
    <meta property="og:description" content="{esc_attr(desc)}" />
    <meta property="og:image" content="{img}" />
    <meta property="og:image:type" content="image/png" />
    <meta property="og:image:width" content="1200" />
    <meta property="og:image:height" content="630" />
    <meta property="og:image:alt" content="{esc_attr(alt)}" />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:title" content="{esc_attr(title)}" />
    <meta name="twitter:description" content="{esc_attr(desc)}" />
    <meta name="twitter:image" content="{img}" />
    <meta name="twitter:image:alt" content="{esc_attr(alt)}" />

    <script type="application/ld+json">
{ld}
    </script>
  </head>
  <body>
    <a class="skip" href="#content">Skip to the comparison</a>

    <!-- partial:topbar -->
    <!-- /partial:topbar -->

    <main class="doc" id="content" tabindex="-1">'''

TAIL = '''    </main>

    <!-- partial:footer -->
    <!-- /partial:footer -->
  </body>
</html>
'''

def ldjson(graph):
  s = json.dumps({"@context":"https://schema.org","@graph":graph}, indent=2, ensure_ascii=False)
  return "\n".join("      "+l for l in s.split("\n"))

def crumbs(items):
  out=['        <ol class="crumbs">']
  for name,href in items:
    out.append(f'          <li><a href="{href}">{name}</a></li>' if href else f'          <li aria-current="page">{name}</li>')
  out.append('        </ol>'); return "\n".join(out)

def bc_ld(items):
  return {"@type":"BreadcrumbList","itemListElement":[dict({"@type":"ListItem","position":i+1,"name":n}, **({"item":SITE+h} if h else {})) for i,(n,h) in enumerate(items)]}

def stamp(what):
  return f'        <p class="stamp"><span class="tag">Updated <time datetime="{DATE}">{DATE_TEXT}</time></span> Checked against {what}. <a href="#sources">Sources</a></p>'

SMOL_SRC = ("smolcast, main branch", "https://github.com/Archetypically/smolcast", "github.com/Archetypically/smolcast")

def footnotes(sources):
  items = "".join(f'<li>{name}: <a href="{url}">{label}</a></li>' for name,url,label in [SMOL_SRC]+sources)
  return f'''      <footer class="footnotes" id="sources" aria-labelledby="sources-title">
        <h2 id="sources-title">Sources</h2>
        <ol>{items}</ol>
        <p>Checked against these versions on <time datetime="{DATE}">{DATE_TEXT}</time>, which is also when this page was last updated. Found something stale? <a href="https://github.com/Archetypically/smolcast/issues">Open an issue</a>.</p>
      </footer>'''

def card(name, by, ts, best, us=False, href=None, visit=None):
  icon = ICON if us else ""
  links = []
  if href: links.append(f'<a href="{href}">Read the comparison</a>')
  if visit: links.append(f'<a href="{visit}">Get {name}</a>')
  more = f'\n          <p class="links">{"".join(links)}</p>' if links else ""
  return f'''        <article class="card{' us' if us else ''}">
          <h2>{icon}{name}</h2>
          <p class="by">{by}</p>
          {tags(ts)}
          <p class="best"><b>Best for</b> {best}</p>{more}
        </article>'''

def curl(text):
  # Typographic apostrophes in text, never inside tags or the JSON-LD block.
  top, sep, rest = text.partition("</script>")
  return top + sep + re.sub(r">[^<]+<", lambda m: re.sub(r"(?<=\w)'(?=\w)", "\u2019", m.group(0)), rest)
def write(path, text):
  with open(path, "w", encoding="utf-8") as f:
    f.write(curl(text))

os.makedirs(os.path.join(OUT,"compare"), exist_ok=True)
for p in PAGES:
  path=f"/compare/{p['slug']}"
  bc=[("smolcast","/"),("Alternatives","/compare/"),(p["name"],None)]
  us = marked(p["us_wins"])
  graph=[{"@type":"WebPage","@id":SITE+path,"url":SITE+path,"name":p["title"],"description":p["desc"],
    "datePublished":DATE,"dateModified":DATE,"isPartOf":{"@id":SITE+"/#website"},
    "about":[{"@id":SITE+"/#app"},{"@type":"SoftwareApplication","name":p["full"],"url":p["url"]}],
    "author":{"@id":SITE+"/#author"}}, bc_ld(bc)]
  body=[head(p["title"],p["desc"],path,ldjson(graph),alt=f"smolcast vs {p['full']}: account, clipboard history, snippets, emoji, extensions, AI and download, side by side."),
   '      <header class="doc-hero">', crumbs(bc), f'        <h1>smolcast vs {p["full"]}</h1>',
   f'        <p class="lead">{p["lead"]}</p>', stamp(p["checked"]), '      </header>', '',
   '      <div class="cards">', card("smolcast","Open source",p["us_tags"],p["us_best"],us=True), card(p["full"],p["them_by"],p["them_tags"],p["them_best"],visit=p["visit"]), '      </div>','',
   '      <section class="doc-section" aria-labelledby="table-title">','        <h2 id="table-title">Feature by feature</h2>',
   '        <p>Bold marks where smolcast comes out ahead, and only where the other side publishes the fact.</p>',
   '        <div class="table-wrap">', table([("smolcast",us),(p["full"],p["them"])]), '        </div>','      </section>','',
   '      <section class="doc-section" aria-labelledby="choose-title">','        <h2 id="choose-title">Pick the one that fits</h2>','        <div class="choose">',
   '          <div class="card us"><h3>Go with smolcast if</h3><ul class="checks">'+"".join(f"<li>{x}</li>" for x in p["us_if"])+'</ul></div>',
   f'          <div class="card"><h3>Go with {p["name"]} if</h3><ul class="checks">'+"".join(f"<li>{x}</li>" for x in p["them_if"])+f'</ul><p class="links"><a href="{p["visit"]}">Get {p["full"]}</a></p></div>',
   '        </div>', f'        <p class="related">{RELATED[p["slug"]]}</p>', '      </section>','',
   '      <section class="doc-section" aria-labelledby="moving-title">',f'        <h2 id="moving-title">Coming from {p["name"]}</h2>','        <ul class="maps">']
  body += [f'          <li><span class="from">{a}</span><span>{b}</span></li>' for a,b in p["maps"]]
  body += ['        </ul>','      </section>','',CTA,'',
   '      <section class="doc-section" aria-labelledby="more-title">','        <h2 id="more-title">More alternatives</h2>', rows_html(p["slug"]),'      </section>','',footnotes(p["sources"]), TAIL]
  write(os.path.join(OUT,"compare",p["slug"]+".html"), "\n".join(body))

# hub
HUB=PAGES[:3]
path="/compare/"
title="Windows launcher alternatives, compared"
desc=f"smolcast, Raycast for Windows, PowerToys Command Palette and Flow Launcher side by side, with one-on-one pages for {len(PAGES)} alternatives: price, extensions, AI, clipboard history, snippets and size."
bc=[("smolcast","/"),("Alternatives",None)]
graph=[{"@type":"CollectionPage","@id":SITE+path,"url":SITE+path,"name":title,"description":desc,"datePublished":DATE,"dateModified":DATE,
  "isPartOf":{"@id":SITE+"/#website"},"about":[{"@id":SITE+"/#app"}]+[{"@type":"SoftwareApplication","name":p["full"],"url":p["url"]} for p in HUB],
  "hasPart":[{"@id":SITE+f"/compare/{p['slug']}"} for p in PAGES],
  # The same pages, in the order the visible "Alternatives, one on one" list gives them.
  "mainEntity":{"@type":"ItemList","name":"smolcast alternatives","numberOfItems":len(PAGES),
    "itemListElement":[{"@type":"ListItem","position":i+1,"name":f"smolcast vs {p['full']}","url":SITE+f"/compare/{p['slug']}"} for i,p in enumerate(PAGES)]}}, bc_ld(bc)]
hubus = marked(["size"])
# On the hub, as on every page, only smolcast's wins are bold.
cards=[card("smolcast","Open source",["Free","Open source","7 MB","No account"],"the essentials, small and private",us=True)]
for p in HUB: cards.append(card(p["full"],p["them_by"],p["them_tags"],p["them_best"],href=f"/compare/{p['slug']}",visit=p["visit"]))
HUBROWS=[("The basics",[("price","Price"),("license","Source"),("os","Runs on"),("account","Account")]),
 ("Features",[("clip","Clipboard history"),("snip","Snippets"),("emoji","Emoji picker"),("files","File search"),("calc","Calculator"),("ext","Extensions"),("ai","AI"),("win","Window management")]),
 ("Under the hood",[("size","Download")])]
body=[head(title,desc,path,ldjson(graph),"website",alt="Windows launcher alternatives compared: smolcast, Raycast for Windows, PowerToys Command Palette and Flow Launcher, feature by feature."),
 '      <header class="doc-hero">', crumbs(bc), f'        <h1>{title}</h1>',
 '        <p class="lead">smolcast and three alternatives, from one small app to a full extension platform.</p>',
 f'        <p class="stamp"><span class="tag">Updated <time datetime="{DATE}">{DATE_TEXT}</time></span> Checked against each project\'s own site. smolcast is ours, so the <a href="#sources">sources</a> are below.</p>',
 '      </header>','','      <div class="cards">', *cards, '      </div>','',
 '      <section class="doc-section" aria-labelledby="table-title">','        <h2 id="table-title">Feature by feature</h2>',
 '        <p>Bold marks where smolcast comes out ahead of all three, and only on published facts.</p>',
 '        <div class="table-wrap">', table([("smolcast",hubus)]+[(p["full"],p["them"]) for p in HUB], HUBROWS, wide=True), '        </div>','      </section>','',
 '      <section class="doc-section" aria-labelledby="mac-title">','        <h2 id="mac-title">Tinycast and Alfred are the Mac cousins</h2>',
 '        <p>Tinycast is free, open source and Mac only, and Alfred is the long-standing Mac favorite. smolcast is the closest Windows alternative to either: <a href="/compare/tinycast">smolcast vs Tinycast</a>, <a href="/compare/alfred">smolcast vs Alfred</a>.</p>','      </section>','',
 CTA,'','      <section class="doc-section" aria-labelledby="more-title">','        <h2 id="more-title">Alternatives, one on one</h2>', rows_html("hub"),'      </section>','',footnotes(list(dict.fromkeys(src for p in PAGES for src in p["sources"]))), TAIL]
write(os.path.join(OUT,"compare","index.html"), "\n".join(body))
print("wrote public/compare/")
