# FreeDataDiet

> **Put your storage on a diet.**  
> A fast, zero-bloat disk usage analyzer built in native Rust & egui.  
> *Find what is eating your gigabytes without subscriptions, ads, or background services.*

[![Version: 1.2.0](https://img.shields.io/badge/version-1.2.0-blue.svg)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Platform: Windows | Linux | macOS](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-0078D6.svg)]()
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust%20%26%20egui-DEA584.svg)]()
[![Single Binary](https://img.shields.io/badge/Binary-Portable%20%28No%20Install%29-brightgreen.svg)]()

---

## Downloads & Installation

Pre-compiled, zero-dependency release packages are published on the **[Releases](https://github.com/LycosLane/FreeDataDiet/releases)** page.

### Windows
* **Winget (Recommended):**
  ```cmd
  winget install freedatadiet
  ```
* **Standalone Executable (`freedatadiet-<version>-windows-x64.exe`):** Single portable file. Zero installer, double-click and run.

### Linux
* **Official APT Repository (Debian / Ubuntu / Mint / Pop!_OS — Recommended):**  
  ```bash
  # 1. Add GPG signing key
  curl -fsSL https://lycoslane.github.io/freedatadiet/pubkey.gpg | sudo gpg --dearmor -o /etc/apt/keyrings/freedatadiet.gpg

  # 2. Add repository source
  echo "deb [signed-by=/etc/apt/keyrings/freedatadiet.gpg] https://lycoslane.github.io/freedatadiet stable main" | sudo tee /etc/apt/sources.list.d/freedatadiet.list

  # 3. Install & automatically stay up to date
  sudo apt update && sudo apt install freedatadiet
  ```
* **Arch Linux (AUR):**
  ```bash
  yay -S freedatadiet-bin
  ```
* **Universal AppImage (`FreeDataDiet-<version>-x86_64.AppImage`):** Single-file standalone executable for any Linux distribution.
* **Direct Debian Package (`.deb`):** `sudo apt install ./freedatadiet_*_amd64.deb`.

### macOS
* **Apple Disk Image (`FreeDataDiet-<version>-macos.dmg`):** Open the `.dmg` and drag `FreeDataDiet.app` into your `Applications` folder.

---

### In-App Update Checker
FreeDataDiet includes a built-in update indicator. When a new version is released, a green **`⟳ Update`** icon appears in the top navigation bar. Clicking it allows 1-click download & restart for portable executables, or provides the quick update command for your package manager.

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
7. **Safe Deletion:** Sends items to the native Recycle Bin / Trash by default, with explicit confirmation for permanent deletion.

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
| **Right Arrow** | Enter selected folder |
| **Enter** | Enter selected folder or open selected file |
| **Backspace / Left** | Navigate up to parent directory |
| **A-Z, 0-9 (Quick Search)** | Jump to name (Explorer style) or live filter visible rows |
| **F2** | Cycle search mode (Jump to Name vs Live Filter) |
| **F3** | Activate / toggle Live Filter search bar |
| **Esc** | Clear search filter or close open dialogs |
| **Delete** | Move selected item to Recycle Bin / Trash |
| **Shift + Delete** | Permanently delete selected item (with confirmation) |
| **Context Menu / Shift+F10** | Open right-click context menu using keyboard |
| **Ctrl + H** | Toggle hidden files and folders |
| **Ctrl + Shift + H** | Toggle protected system files and NTFS junctions |
| **F12  /  Ctrl + ,** | Open / close Settings & Preferences dialog |
| **F1  /  ?** | Open / close Keyboard Shortcuts cheat sheet |
| **F5** | Refresh and rescan current directory |
| **F11** | Toggle fullscreen mode |
| **Double Click** | Enter directory or open file |
| **Right Click** | Context menu (Open, Show in file manager, Copy name / path, Properties on Windows, Trash, Permanent Delete) |
| **Header Click** | Sort by column (click again to invert, 3rd click resets) |
| **Header Drag & Drop** | Reorder table columns interactively |
| **`Restart as Admin`** | Elevate to Administrator permissions |

---

## Features & Highlights

* **Live Discovery:** Folders appear immediately upon scan start with live size calculations.
* **Type Badges:** Aligned badges (`[DIR]`, `[SYS]`, `[LINK]`, `[LOCK]`, `[PART]`, `[FILE]`) keep names aligned. Hover a badge for its meaning.
* **Themes:** Built-in Dark Mode, Light Mode, and automatic OS system theme detection.
* **Usage Bar Scaling:** Scale bars relative to the largest item in view, current folder total, used drive space, or full disk capacity.
* **Quick Search Modes:** Jump to files by typing a prefix (Explorer style) or live filter matching rows (FilePilot style).
* **Keyboard Navigation:** Full keyboard support across table rows, menus, and delete confirmation modals.
* **Stable Selection:** Selection remains anchored to the chosen item even as background updates re-sort rows.

---

## Configuration (`config.json`)

Settings persist in a human-readable `config.json`. It lives next to the executable if one exists there (portable use, default on Windows when the folder is writable), otherwise in your user config folder (`%APPDATA%\FreeDataDiet`, `~/.config/FreeDataDiet`, `~/Library/Application Support/FreeDataDiet`):
* Theme (`"Dark"`, `"Light"`, `"System"`)
* Usage bar scaling (`"LargestItem"`, `"Folder"`, `"DriveUsed"`, `"DriveTotal"`)
* Column order
* Multi-column sort rules
* Show hidden / system / locked files toggles
* Quick search mode (`"JumpToPrefix"`, `"LiveFilter"`, `"Disabled"`)
* Remember last visited path toggle (`remember_last_path`)
* Custom exclusion filter list & enabled state
* Granular per-key shortcut toggles (`f2_search_mode`, `f3_search_filter`, `f5_refresh`, `f11_fullscreen`, `f12_settings`, `ctrl_comma_settings`, `ctrl_h_hidden`, `ctrl_shift_h_system`, `del_trash`, `shift_del_permanent`)

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
