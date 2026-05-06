use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Stage upstream lwext4 (`vendor/lwext4`, a real submodule
    // pointing at gkostka/lwext4) into the build OUT_DIR, then
    // apply our patches/ directory on top. Compiling out of
    // OUT_DIR keeps the submodule pristine and lets developers
    // run `git diff` against upstream cleanly.
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo"));
    let staged = out_dir.join("lwext4-staged");
    if staged.exists() {
        fs::remove_dir_all(&staged).expect("clean staging dir");
    }
    let upstream_root = PathBuf::from("vendor/lwext4");
    if !upstream_root.join("src/ext4_mkfs.c").exists() {
        panic!(
            "ext4-lwext4-sys: vendor/lwext4 submodule not initialized. \
             Run `git submodule update --init --recursive` (the parent \
             agx-ext4-lwext4 repo pins this to gkostka/lwext4@58bcf89)."
        );
    }
    copy_dir(&upstream_root, &staged);
    apply_patches(&staged);

    let lwext4_src = staged.join("src");
    let lwext4_inc = staged.join("include");

    // Core source files (BSD-3-Clause / MIT OR Apache-2.0 compatible)
    let mut sources = vec![
        "ext4.c",
        "ext4_balloc.c",
        "ext4_bcache.c",
        "ext4_bitmap.c",
        "ext4_block_group.c",
        "ext4_blockdev.c",
        "ext4_crc32.c",
        "ext4_debug.c",
        "ext4_dir.c",
        "ext4_dir_idx.c",
        "ext4_fs.c",
        "ext4_hash.c",
        "ext4_ialloc.c",
        "ext4_inode.c",
        "ext4_journal.c",
        "ext4_mkfs.c",
        "ext4_super.c",
        "ext4_trans.c",
    ];

    // GPL-2.0 licensed files - only included with explicit feature flags
    if env::var("CARGO_FEATURE_GPL_EXTENTS").is_ok() {
        sources.push("ext4_extent.c");
    }
    if env::var("CARGO_FEATURE_GPL_XATTR").is_ok() {
        sources.push("ext4_xattr.c");
    }

    let mut build = cc::Build::new();

    for src in &sources {
        build.file(lwext4_src.join(src));
    }

    // Helper functions (sizeof, etc.)
    build.file("vendor/helpers.c");

    build
        .include(&lwext4_inc)
        .include(lwext4_inc.join("misc"))
        .define("CONFIG_USE_DEFAULT_CFG", "1")
        .define("CONFIG_HAVE_OWN_OFLAGS", "1");

    // Disable extent support at the C preprocessor level when the GPL feature
    // is not enabled. Without this, ext4_fs.c compiles calls to extent functions
    // (guarded by CONFIG_EXTENTS_ENABLE) but ext4_extent.c is not compiled,
    // causing undefined symbol errors at link time.
    if env::var("CARGO_FEATURE_GPL_EXTENTS").is_err() {
        build.define("CONFIG_EXTENTS_ENABLE", "0");
    }

    build
        .flag_if_supported("-std=c99")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-sign-compare")
        .warnings(false)
        .compile("lwext4");

    println!("cargo:rerun-if-changed=vendor/lwext4/src");
    println!("cargo:rerun-if-changed=vendor/lwext4/include");
    println!("cargo:rerun-if-changed=patches");
}

/// Recursive copy. Skips `.git` directories so the submodule
/// metadata doesn't end up in OUT_DIR.
fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create staged dir");
    for entry in fs::read_dir(src).expect("read upstream dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let ft = entry.file_type().expect("file type");
        if ft.is_dir() {
            copy_dir(&from, &to);
        } else if ft.is_symlink() {
            // Realise symlinks in case the patch needs to
            // operate on a regular file.
            let target = fs::read_link(&from).expect("read link");
            let resolved = if target.is_absolute() {
                target
            } else {
                from.parent().unwrap().join(&target)
            };
            fs::copy(&resolved, &to).expect("copy symlink target");
        } else {
            fs::copy(&from, &to).expect("copy file");
        }
    }
}

/// Apply every `*.patch` under `patches/` (sorted by name) to
/// the staged source via `patch -p1`. The patches live OUTSIDE
/// the submodule so the submodule stays pristine — diff'ing
/// our fork against upstream is just `ls patches/`.
fn apply_patches(staged: &Path) {
    let patches_dir = PathBuf::from("patches");
    if !patches_dir.exists() {
        return;
    }
    let mut entries: Vec<_> = fs::read_dir(&patches_dir)
        .expect("read patches dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x == "patch")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let patch = entry.path();
        let abs_patch = patch
            .canonicalize()
            .expect("canonicalize patch path");
        let status = Command::new("patch")
            .arg("-p1")
            .arg("-i")
            .arg(&abs_patch)
            .current_dir(staged)
            .status()
            .expect("spawn patch(1)");
        if !status.success() {
            panic!(
                "ext4-lwext4-sys: failed to apply {}: patch exited with {status}",
                patch.display()
            );
        }
    }
}
