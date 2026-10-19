//! Compiles the real `driver/r9t_fan/r9t_fan.c` against an in-memory EC and workqueue
//! (`driver/r9t_fan/test/stubs.h`) and runs the fault/lifecycle scenarios in
//! `driver/r9t_fan/test/harness.c`. No module is loaded and no hardware is touched.
//! Needs a C compiler (`cc`); skipped with a note when there is none.

use std::path::Path;
use std::process::Command;

#[test]
fn real_driver_faults_and_lifecycle() {
    let drv = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../driver/r9t_fan").canonicalize().unwrap();
    if Command::new("cc").arg("--version").output().is_err() {
        eprintln!("cc yok: r9t_fan C hata testi atlandı");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("linux")).unwrap();
    // The driver includes <linux/*.h>; module.h pulls in the stubs, the rest are empty.
    for name in ["module", "dmi", "kernel", "workqueue", "mutex", "suspend", "delay"] {
        let body = if name == "module" { format!("#include \"{}\"\n", drv.join("test/stubs.h").display()) } else { String::new() };
        std::fs::write(root.join(format!("linux/{name}.h")), body).unwrap();
    }
    let main = root.join("main.c");
    std::fs::write(&main, format!("#include \"{}\"\n#include \"{}\"\n", drv.join("r9t_fan.c").display(), drv.join("test/harness.c").display())).unwrap();
    let bin = root.join("harness");
    let cc = Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter", "-Wno-unused-variable", "-Wno-unused-function", "-I"])
        .arg(root)
        .arg(&main)
        .arg("-o")
        .arg(&bin)
        .output()
        .unwrap();
    assert!(cc.status.success(), "derleme başarısız:\n{}", String::from_utf8_lossy(&cc.stderr));
    let run = Command::new(&bin).output().unwrap();
    assert!(run.status.success(), "senaryo başarısız:\n{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
    assert!(String::from_utf8_lossy(&run.stdout).contains("all scenarios passed"));
}
