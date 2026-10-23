# FreeDataDiet

> Put your storage on a diet. A minimalist, lightning-fast cross-platform disk usage analyzer.  
> *No subscriptions. No nag screens. Pure, transparent disk diagnostics.*

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform: Windows | Linux | macOS](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-0078D6.svg)]()
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust%20%26%20egui-DEA584.svg)]()
[![Zero Bloatware](https://img.shields.io/badge/Bloatware-0%25-brightgreen.svg)]()

```text
              .
             / \
         ___/___\___
        /     |     `---.
    ===+===   |          \
    |     |   |        ===+===
   [  GB   ]  |        |     |
   \_______/ ===      [  MB   ]   FreeDataDiet
            /   \     \_______/   Put your storage on a diet.
           =======                Find what's eating your gigabytes.
```

---

## Why FreeDataDiet?

Traditional disk analyzers like *TreeSize Free* wrap basic filesystem inspection in commercial nagware, lock essential features behind 50€/year subscriptions, and push upgrade banners onto users.

**FreeDataDiet** is built on a different philosophy:
1. **100% Free & Open Source:** Zero paywalls, zero ads, zero telemetry.
2. **Lightning Fast:** Written in Rust with parallel multithreaded directory traversal. Scans hundreds of thousands of files in seconds.
3. **True Single Binary:** A portable executable with no runtime dependencies (no Electron, no webview, no background services).
4. **Smart Exclude Filter:** Instantly skip developer caches (`.git`, `node_modules`, `target`, `.cache`, `.venv`) to save scan time and focus on what can actually be cleaned.
5. **Effective Storage Awareness:** Displays both nominal file size (*Apparent Size*) and actual block allocation on disk (*Size on Disk* / Sparse Files / Cluster Alignment).
6. **Built-in Safety Guards:** System-critical directories (`C:\Windows`, `/boot`, `/etc`) are protected with visual badges and warnings against accidental deletion.
7. **Safe Delete:** Files are moved to the native OS Recycle Bin by default; permanent deletion requires explicit confirmation.

---

## Companion Tool

* **FreeDataDiet:** Analyze and trim overweight folders and files.
* **FreeDopplerDiet:** Find and eliminate duplicate file clones.
* **FreeWinly:** Pure, bloatware-free Windows maintenance engine.

---

## Keyboard & Navigation Shortcuts

| Key / Action | Function |
| :--- | :--- |
| **Up / Down** | Navigate selection through folder rows |
| **Home / End** | Jump to top / bottom of current listing |
| **PageUp / PageDown** | Scroll up / down by 15 items |
| **Right Arrow** | Navigate into selected folder |
| **Enter** | Enter selected directory or launch file |
| **Backspace / Left** | Navigate up to parent directory |
| **Typing Letters (Quick Search)** | Jump to item by name (Explorer style) or live filter visible rows (FilePilot style, configurable in Settings) |
| **Esc** | Clear quick search filter or dismiss popups |
| **Delete** | Move selected item to Recycle Bin / Trash (with keyboard-navigable confirmation modal) |
| **Shift + Delete** | Permanently delete selected item (with confirmation) |
| **Context Menu / Shift+F10** | Open context menu for selected item using the keyboard |
| **Ctrl + H** | Toggle hidden files and folders |
| **Ctrl + Shift + H** | Toggle protected system files and NTFS junctions |
| **Ctrl + ,  /  Alt + S** | Open Settings & Preferences dialog |
| **F1  /  ?** | Toggle Keyboard Shortcuts cheat sheet (closes on any key press) |
| **F5** | Rescan / refresh current directory |
| **Double Click** | Enter directory or open file |
| **Right Click** | Context menu (Open, Explorer, Copy Path, Properties, Trash, Permanent Delete) |
| **Header Drag & Drop** | Reorder table columns interactively (saved to `config.json`) |
| **Header Click** | 3-state multi-column sort: 1st click = default direction, 2nd click = inverted direction, 3rd click = remove sort rule |
| **`Restart as Admin`** | Elevate to Administrator permissions seamlessly |

---

## Features & Highlights

* **Immediate Discovery:** Shows all direct child folders immediately upon scan start, marking calculating folders with `[calculating]` / `Scanning...` so large folders like `Users` or `Windows` are never hidden while counting.
* **Dedicated Type Column:** Clean `Type` column with aligned badges (`[DIR]`, `[SYS]`, `[JUNC]`, `[FILE]`) keeping file and directory names strictly aligned.
* **Dark & Light Modes:** Built-in Dark Mode, Light Mode, and automatic OS System theme detection with crisp contrast for badges, rows, and headers.
* **Proportional & Relative Usage Bars (%):** Choose between 4 distinct visual scaling modes:
  - *Largest Item in View (100% = biggest file/folder)*: The heaviest entry occupies the full 100% bar, and other items scale in proportion.
  - *Current Folder Total (100% = sum of folder)*: Shows each item's share of the directory's total weight.
  - *Drive Used Space (100% = all used storage)*: Shows share relative to total occupied disk space.
  - *Drive Total Capacity (100% = full disk size)*: Shows share relative to the partition's entire capacity.
* **Instant Keyboard Shortcuts Sheet (F1):** Press `F1` or `?` anytime for an instant shortcut cheat sheet modal that dismisses automatically on any key press.
* **Quick Search Modes:** Choose between Windows Explorer-style *Jump-to-prefix* (cycles on repeated keys) or FilePilot-style *Live filtering* with instant feedback banner.
* **Geometric Sort Arrows:** Crisp vector triangles drawn directly to guarantee identical baseline, dimensions, and zero unicode font fallback glitches.
* **Hidden & System File Toggles:** Easily filter or reveal hidden and system-protected operating system items (such as `hiberfil.sys`, `pagefile.sys`, and NTFS junctions like `Documents and Settings`).
* **Multi-Column Sorting:** Chain sorting criteria across multiple columns with visual priority badges (`Type 1`, `Size 2`).
* **Safe Modal Keyboard Flow:** Delete and Trash modals respond immediately to `Enter` to confirm, `Esc` to cancel, and `Left`/`Right`/`Tab` to switch button focus.
* **Stable Selection:** Selection remains anchored to the chosen item even as background scan updates re-sort rows.

---

## Configuration (`config.json`)

FreeDataDiet stores settings in a clean, human-readable `config.json` next to the executable:
* Theme (`"Dark"`, `"Light"`, `"System"`)
* Usage bar percentage basis (`"LargestItem"`, `"Folder"`, `"DriveUsed"`, `"DriveTotal"`)
* Column order & visibility (including `Type`, `Name`, `Size`, `AllocatedSize`, `Usage`, `Items`, `Modified`)
* Multi-column sort rules
* Show hidden / system files toggles
* Quick search mode (`"JumpToPrefix"`, `"LiveFilter"`, `"Disabled"`)
* Remember last visited path toggle (`remember_last_path`)
* Last visited path (`last_path`)
* Dev cache exclusion list & enabled state

---

## Building from Source

```bash
# Clone the repository
git clone https://github.com/LycosLane/FreeDataDiet.git
cd FreeDataDiet

# Build release executable
cargo build --release

# Run
./target/release/freedatadiet
```

---

## License

MIT License © 2026 LycosLane
