# FreeDataDiet 1.12.0

Put your storage on a diet. A fast, lightweight, and completely portable disk usage analyzer built with Rust and egui.

---

## What is New in Version 1.12.0

### 1. Multi-Pass Duplicate Finder & Smart Hashing
- **Three-Stage Verification:** Identifies candidate files by size, verifies 64 KB prefix and suffix hashes, and computes exact SHA-256 hashes only when necessary.
- **Dynamic File Size Threshold:** Minimum file size filter can be adjusted on the fly during hashing without restarting the scan.
- **Rule-Based Selection:** Auto-select redundant copies via "Keep First", "Keep Oldest", or "Keep Newest" across all groups or filtered views.
- **Category Duplicate Rules:** Configurable rules per file category (Images, Videos, Audio, Documents, Archives).
- **Folder Pattern Suggestions:** Automatically detects sync and backup folder pairs (e.g. C:\Keep vs C:\Sync) and suggests batch selections.

### 2. Multi-Token Live Filter System
- **Explorer & Duplicate Filtering:** Instantly filter folder contents or duplicate clusters as you type.
- **Group Exclusion (-term):** Type `-backup` to hide all duplicate groups that contain a copy in that folder.
- **Path Exclusion (--term):** Type `--backup` to hide only the specific copies located in that folder while keeping other duplicate copies visible.
- **Escaped Minus Searches:** Type `\-` to search for literal hyphens, `-\-` to exclude terms with hyphens, and `---` to exclude all paths with hyphens.
- **Hardened Input Bounds:** Filter buffers are strictly bounded to prevent UI freezes on massive clipboard pastes.

### 3. Explorer Views, Navigation & UI Controls
- **Live UI Scaling:** Smooth UI zoom slider from 80% to 200% with immediate scaling and keyboard shortcut support (Ctrl + Plus / Minus / 0).
- **Full Keyboard Navigation:** Complete table control via Arrow keys, Home, End, PageUp, PageDown, Enter, Space, and Backspace.
- **Unified Context Menu:** Accessible via mouse right-click or keyboard (Menu / F10) with Open, Show in File Manager, Favorites, Path Copying, Permanent Scan Exclusions, and Deletion.
- **System File Separation:** Allows inspecting system directories like Program Files, Windows, and ProgramData in Explorer views while strictly maintaining deletion safety gates.

### 4. Fully Standalone Portable Binary (Windows)
- **Static C-Runtime Linking:** The Windows binary is built with statically linked CRT (+crt-static). It runs out-of-the-box on clean Windows installations without requiring Microsoft Visual C++ Redistributables.

---

## Fixes & Hardening Summary
- **Deletion Safety:** Batch delete and bulk actions strictly honor active path exclusion filters (--term).
- **Typing Boundaries:** Fixed real-time filter reset on incomplete escape tokens (-\ and --\).
- **Path Matching:** Replaced over-broad substring matching on critical Windows system paths.
- **Performance:** Eliminated frame-loop cache invalidations and memory re-allocations in duplicate tables.
- **Architecture:** Modularized context menu, keyboard navigation, and test suites to maintain structural code standards.
