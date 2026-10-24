#!/usr/bin/env python3
"""
APT Repository Builder for FreeDataDiet (GitHub Pages)
Builds Packages, Packages.gz, Release, InRelease, and Release.gpg
"""
import os
import sys
import io
import gzip
import hashlib
import tarfile
import subprocess
from datetime import datetime, timezone

def read_deb_control(deb_path):
    with open(deb_path, "rb") as f:
        magic = f.read(8)
        if magic != b"!<arch>\n":
            raise ValueError(f"Invalid deb file (bad ar magic): {deb_path}")
        while True:
            header = f.read(60)
            if len(header) < 60:
                break
            name = header[:16].decode("latin1").strip().rstrip("/")
            size = int(header[48:58].decode("latin1").strip())
            content = f.read(size)
            if size % 2 == 1:
                f.read(1)  # ar padding
            if name.startswith("control.tar"):
                with tarfile.open(fileobj=io.BytesIO(content)) as tar:
                    for member in tar.getmembers():
                        if member.name.endswith("control") and not member.isdir():
                            f_ctrl = tar.extractfile(member)
                            if f_ctrl:
                                return f_ctrl.read().decode("utf-8")
    raise ValueError(f"No control file found in {deb_path}")

def compute_hashes(data_bytes):
    return {
        "md5": hashlib.md5(data_bytes).hexdigest(),
        "sha1": hashlib.sha1(data_bytes).hexdigest(),
        "sha256": hashlib.sha256(data_bytes).hexdigest(),
        "size": len(data_bytes),
    }

def build_repo(repo_root):
    pool_dir = os.path.join(repo_root, "pool", "main")
    dists_dir = os.path.join(repo_root, "dists", "stable")
    binary_dir = os.path.join(dists_dir, "main", "binary-amd64")
    os.makedirs(binary_dir, exist_ok=True)
    os.makedirs(pool_dir, exist_ok=True)

    packages_entries = []
    for fname in sorted(os.listdir(pool_dir)):
        if fname.endswith(".deb"):
            deb_path = os.path.join(pool_dir, fname)
            with open(deb_path, "rb") as f:
                deb_bytes = f.read()
            hashes = compute_hashes(deb_bytes)
            control = read_deb_control(deb_path).strip()

            rel_filename = f"pool/main/{fname}"
            entry = f"{control}\n"
            entry += f"Filename: {rel_filename}\n"
            entry += f"Size: {hashes['size']}\n"
            entry += f"MD5sum: {hashes['md5']}\n"
            entry += f"SHA1: {hashes['sha1']}\n"
            entry += f"SHA256: {hashes['sha256']}\n"
            packages_entries.append(entry)

    packages_content = "\n\n".join(packages_entries) + "\n" if packages_entries else ""
    packages_bytes = packages_content.encode("utf-8")
    packages_path = os.path.join(binary_dir, "Packages")
    with open(packages_path, "wb") as f:
        f.write(packages_bytes)

    packages_gz_bytes = gzip.compress(packages_bytes, compresslevel=9)
    packages_gz_path = os.path.join(binary_dir, "Packages.gz")
    with open(packages_gz_path, "wb") as f:
        f.write(packages_gz_bytes)

    pkg_h = compute_hashes(packages_bytes)
    pkg_gz_h = compute_hashes(packages_gz_bytes)

    now_rfc2822 = datetime.now(timezone.utc).strftime("%a, %d %b %Y %H:%M:%S +0000")

    release_content = f"""Origin: FreeDataDiet
Label: FreeDataDiet
Suite: stable
Codename: stable
Architectures: amd64
Components: main
Description: Official APT repository for FreeDataDiet disk usage analyzer
Date: {now_rfc2822}
MD5Sum:
 {pkg_h['md5']} {pkg_h['size']} main/binary-amd64/Packages
 {pkg_gz_h['md5']} {pkg_gz_h['size']} main/binary-amd64/Packages.gz
SHA1:
 {pkg_h['sha1']} {pkg_h['size']} main/binary-amd64/Packages
 {pkg_gz_h['sha1']} {pkg_gz_h['size']} main/binary-amd64/Packages.gz
SHA256:
 {pkg_h['sha256']} {pkg_h['size']} main/binary-amd64/Packages
 {pkg_gz_h['sha256']} {pkg_gz_h['size']} main/binary-amd64/Packages.gz
"""
    release_path = os.path.join(dists_dir, "Release")
    with open(release_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(release_content)

    print(f"Generated Packages ({len(packages_entries)} packages) and Release metadata.")

def sign_repo(repo_root, gpg_bin="gpg", key_id=None, homedir=None):
    dists_dir = os.path.join(repo_root, "dists", "stable")
    release_path = os.path.join(dists_dir, "Release")
    inrelease_path = os.path.join(dists_dir, "InRelease")
    release_gpg_path = os.path.join(dists_dir, "Release.gpg")

    if os.path.exists(inrelease_path):
        os.remove(inrelease_path)
    if os.path.exists(release_gpg_path):
        os.remove(release_gpg_path)

    base_cmd = [gpg_bin, "--batch", "--yes"]
    if homedir:
        base_cmd += ["--homedir", homedir]
    if key_id:
        base_cmd += ["--default-key", key_id]

    # Clearsign InRelease
    cmd1 = base_cmd + ["--armor", "--clearsign", "-o", inrelease_path, release_path]
    subprocess.run(cmd1, check=True)

    # Detached sign Release.gpg
    cmd2 = base_cmd + ["--armor", "--detach-sign", "-o", release_gpg_path, release_path]
    subprocess.run(cmd2, check=True)

    print("Successfully signed InRelease and Release.gpg.")

if __name__ == "__main__":
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    build_repo(root)
    key_id = os.environ.get("APT_GPG_KEY_ID")
    gpg_bin = os.environ.get("GPG_BIN", "gpg")
    homedir = os.environ.get("GNUPGHOME")
    if key_id or os.environ.get("SIGN_RELEASE") == "1":
        sign_repo(root, gpg_bin=gpg_bin, key_id=key_id, homedir=homedir)
