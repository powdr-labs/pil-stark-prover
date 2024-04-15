use std::{path::Path, process::Command};

fn main() {
    // Run make test in the zkevm-prover directory
    let zkevm_prover_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("externals/zkevm-prover");

    eprintln!(
        "Running make test in zkevm-prover directory: {}",
        zkevm_prover_dir.display()
    );
    let make_status = Command::new("make")
        .arg("test")
        .arg("-j")
        .arg(num_cpus::get().to_string())
        .current_dir(&zkevm_prover_dir)
        .status()
        .expect("Failed to run make test in zkevm-prover directory");

    assert!(
        make_status.success(),
        "make test failed in zkevm-prover directory"
    );
}
