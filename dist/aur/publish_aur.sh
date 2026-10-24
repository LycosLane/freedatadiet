#!/bin/bash
set -e

# Script to publish or update freedatadiet-bin on AUR (aur.archlinux.org)
# Requirements: AUR SSH access configured (ssh -T aur@aur.archlinux.org)

AUR_REPO="ssh://aur@aur.archlinux.org/freedatadiet-bin.git"
CLONE_DIR="/tmp/freedatadiet-bin-aur"

echo "=== Deploying FreeDataDiet to AUR ==="
rm -rf "$CLONE_DIR"
git clone "$AUR_REPO" "$CLONE_DIR" || (mkdir -p "$CLONE_DIR" && cd "$CLONE_DIR" && git init && git remote add origin "$AUR_REPO")

cp dist/aur/PKGBUILD "$CLONE_DIR/PKGBUILD"
cp dist/aur/.SRCINFO "$CLONE_DIR/.SRCINFO"

cd "$CLONE_DIR"
git add PKGBUILD .SRCINFO
git commit -m "Update freedatadiet-bin" || echo "No changes to commit"
git push origin master
echo "=== Successfully published to AUR! ==="
