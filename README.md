# FreeDataDiet

> **Put your storage on a diet.**  
> A fast, zero-bloat disk usage analyzer built in native Rust & egui.  
> *Find what is eating your gigabytes without subscriptions, ads, or background services.*

[![Version: 0.5.4](https://img.shields.io/badge/version-0.5.4-blue.svg)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Platform: Windows | Linux | macOS](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-0078D6.svg)]()
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust%20%26%20egui-DEA584.svg)]()
[![Single Binary](https://img.shields.io/badge/Binary-Portable%20%28No%20Install%29-brightgreen.svg)]()

---

## Why FreeDataDiet?

Commercial disk analyzers wrap basic filesystem inspection in nagware, lock essential features behind annual subscriptions, and push upgrade banners onto users.

**FreeDataDiet** is built on a simpler philosophy:
1. **Free & Open Source:** No paywalls, no subscriptions, and no telemetry.
2. **Fast & Parallel:** Multi-threaded directory traversal scans hundreds of thousands of files in seconds.
3. **Portable Single Binary:** Runs directly without installers, background services, or webview bloat.
4. **Developer Cache Filter:** Skip `.git`, `node_modules`, `target`, `.cache`, and `.venv` to focus on cleanable user data.
5. **Apparent Size & Size on Disk:** Displays both nominal file size and actual cluster allocation on disk.
6. **System Protection:** Visual badges and warnings protect system-critical directories (`C:\Windows`, `/boot`).
7. **Safe Deletion:** Sends items to the native Recycle Bin by default, with explicit confirmation for permanent deletion.

---

## Companion Tools

* **FreeDataDiet:** Analyze and trim overweight folders and files.
* **FreeDopplerDiet:** Find and eliminate duplicate file clones.
* **FreeWinly:** Bloatware-free Windows maintenance engine.

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
| **Typing Letters (Quick Search)** | Jump to item by name (Explorer style) or live filter visible rows (FilePilot style) |
| **Esc** | Clear search filter or close open dialogs |
| **Delete** | Move selected item to Recycle Bin / Trash |
| **Shift + Delete** | Permanently delete selected item (with confirmation) |
| **Context Menu / Shift+F10** | Open context menu using the keyboard |
| **Ctrl + H** | Toggle hidden files and folders |
| **Ctrl + Shift + H** | Toggle protected system files and NTFS junctions |
| **Ctrl + ,  /  Alt + S** | Open Settings dialog |
| **F1  /  ?** | Open Keyboard Shortcuts cheat sheet |
| **F5** | Refresh and rescan current directory |
| **Double Click** | Enter directory or open file |
| **Right Click** | Context menu (Open, Explorer, Copy Path, Properties, Trash, Permanent Delete) |
| **Header Click** | Sort by column (click again to invert, 3rd click resets) |
| **Header Drag & Drop** | Reorder table columns interactively |
| **`Restart as Admin`** | Elevate to Administrator permissions |

---

## Features & Highlights

* **Live Discovery:** Folders appear immediately upon scan start with live size calculations.
* **Type Badges:** Aligned badges (`[DIR]`, `[SYS]`, `[JUNC]`, `[FILE]`) keep names strictly aligned.
* **Themes:** Built-in Dark Mode, Light Mode, and automatic OS system theme detection.
* **Usage Bar Scaling:** Scale bars relative to the largest item in view, current folder total, used drive space, or full disk capacity.
* **Quick Search Modes:** Jump to files by typing a prefix (Explorer style) or live filter matching rows (FilePilot style).
* **Keyboard Navigation:** Full keyboard support across table rows, menus, and delete confirmation modals.
* **Stable Selection:** Selection remains anchored to the chosen item even as background updates re-sort rows.

---

## Configuration (`config.json`)

Settings persist in a human-readable `config.json` next to the executable:
* Theme (`"Dark"`, `"Light"`, `"System"`)
* Usage bar scaling (`"LargestItem"`, `"Folder"`, `"DriveUsed"`, `"DriveTotal"`)
* Column order & visibility
* Multi-column sort rules
* Show hidden / system files toggles
* Quick search mode (`"JumpToPrefix"`, `"LiveFilter"`, `"Disabled"`)
* Remember last visited path toggle (`remember_last_path`)
* Last visited path (`last_path`)
* Custom exclusion filter list & enabled state

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
