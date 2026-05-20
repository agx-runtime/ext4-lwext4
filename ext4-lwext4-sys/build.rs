use std::env;
use std::path::PathBuf;

fn main() {
    // The vendored lwext4 source lives directly under
    // `vendor/lwext4/`. This is the agx-ext4-lwext4 fork, so
    // any AGX-specific fixes are checked into the tracked
    // source — `git diff` against upstream gkostka/lwext4 is
    // the authoritative changelog. There's no longer a
    // separate `patches/` step (was carried for a brief
    // window when vendor/ was a nested submodule pointing
    // upstream).
    let lwext4_root = PathBuf::from("vendor/lwext4");
    if !lwext4_root.join("src/ext4_mkfs.c").exists() {
        panic!("ext4-lwext4-sys: vendor/lwext4/src/ext4_mkfs.c not found in repo");
    }

    let lwext4_src = lwext4_root.join("src");
    let lwext4_inc = lwext4_root.join("include");

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
}
