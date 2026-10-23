# FreeDataDiet

> Put your storage on a diet. A minimalist, lightning-fast cross-platform disk usage analyzer.  
> *No subscriptions. No nag screens. Pure, transparent disk diagnostics.*

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform: Windows | Linux | macOS](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-0078D6.svg)]()
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust%20%26%20egui-DEA584.svg)]()
[![Zero Bloatware](https://img.shields.io/badge/Bloatware-0%25-brightgreen.svg)]()

```text
       \   /
      .-'-.-'-.
    /`         `\      FreeDataDiet
   |   (o) (o)   |     Put your storage on a diet.
    \   \___/   /      Find what's eating your gigabytes.
     `-..___..-'
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
| **Double Click** | Enter folder |
| **`⬆ Nach oben`** | Navigate to parent directory |
| **`⚙️ Filter`** | Toggle developer cache excludes (`.git`, `node_modules`, etc.) |
| **Sort Headers** | Sort by Name, Apparent Size, Size on Disk, Item Count, or Date |
| **`🛡️ Als Admin`** | Restart with elevated Administrator privileges on demand |
| **`🗑️ Papierkorb`** | Move item to OS Recycle Bin (safe) |
| **`⚠️ Endgültig`** | Permanently delete item (with confirmation) |

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
