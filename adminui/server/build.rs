//! Builds `mesh/wappfix.so`, the LD_PRELOAD shim the EasyMesh support starts
//! the firmware's `wapp` with (see mesh/wappfix.c). It targets the device's
//! glibc 2.26 / soft-float ARM userland, so it is built with `zig cc`. Without
//! zig an empty placeholder is embedded and EasyMesh refuses to start.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let src = PathBuf::from("../mesh/wappfix.c");
    println!("cargo:rerun-if-changed={}", src.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("wappfix.so");
    let ok = Command::new("zig")
        .args(["cc", "-target", "arm-linux-gnueabi.2.26", "-mcpu=arm1176jzf_s", "-shared", "-fPIC", "-Os", "-o"])
        .arg(&out)
        .arg(&src)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        println!("cargo:warning=zig not available: EasyMesh agent shim not built; EasyMesh will refuse to start");
        std::fs::write(&out, []).expect("write placeholder");
    }
}
