use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(not(target_arch = "x86_64"))]
compile_error!("This crate is only available on x86-64 architectures.");

fn recursive_hardlink(from: &Path, to: &Path) {
    let mut stack = vec![from.to_path_buf()];
    while let Some(curr_from) = stack.pop() {
        let to = to.join(curr_from.strip_prefix(from).unwrap());
        eprintln!("Linking {:?} to {:?}", curr_from, to);
        if curr_from.is_dir() {
            std::fs::create_dir(&to).expect("Failed to create directory");
            for entry in curr_from.read_dir().unwrap() {
                stack.push(entry.unwrap().path());
            }
        } else {
            std::fs::hard_link(&curr_from, &to).expect("Failed to hard link file");
        }
    }
}

fn main() {
    let externals_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("externals");
    // Configure build.rs rerun. Lossy conversion to str won't cut it here...
    println!("cargo:rerun-if-changed={}", externals_dir.to_str().unwrap());

    // Clear cargo's OUT_DIR
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    eprintln!("Clearing OUT_DIR: {:?}", out_dir);
    std::fs::remove_dir_all(&out_dir).unwrap();

    // Hardlink all files of the externals directory to OUT_DIR
    recursive_hardlink(&externals_dir, &out_dir);

    // Run make test in the zkevm-prover directory
    let zkevm_prover_dir = out_dir.join("zkevm-prover");
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
