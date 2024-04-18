use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

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
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let externals_dir = src_dir.join("externals");
    let dynamic_chelpers = src_dir.join("resources/dynamic_chelpers.cpp");
    // Configure build.rs rerun.
    println!(
        "cargo:rerun-if-changed={}",
        dynamic_chelpers.to_str().unwrap()
    );
    println!("cargo:rerun-if-changed={}", externals_dir.to_str().unwrap());

    // Clear cargo's OUT_DIR
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    eprintln!("Clearing OUT_DIR: {:?}", out_dir);
    std::fs::remove_dir_all(&out_dir).unwrap();

    // Hardlink all files of the externals directory to OUT_DIR
    recursive_hardlink(&externals_dir, &out_dir);

    // Hardlink the dynamic_chelpers.cpp file to OUT_DIR
    std::fs::hard_link(dynamic_chelpers, out_dir.join("dynamic_chelpers.cpp")).unwrap();

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
