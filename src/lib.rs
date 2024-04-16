use std::{
    borrow::Cow,
    env, fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};

use itertools::Itertools;

pub struct OutputFiles {
    pub verification_key_json: PathBuf,
    pub starkinfo_json: PathBuf,
    pub proof_json: PathBuf,
}

const MAX_NODE_MEM: u32 = 1024 * 16;

fn deps_dir() -> Cow<'static, Path> {
    const DEFAULT_DEPS_DIR: &str = env!("OUT_DIR");

    match env::var_os("PIL_STARK_PROVER_DEPS") {
        Some(deps_dir) => Cow::Owned(deps_dir.into()),
        None => {
            let default_deps_dir = Path::new(DEFAULT_DEPS_DIR);
            if default_deps_dir.is_dir() {
                Cow::Borrowed(default_deps_dir)
            } else {
                panic!("pil-stark-prover dependencies directory not found!\nEither set PIL_STARK_PROVER_DEPS environment variable, or build the project locally from source.\nSee README.md for more information.")
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("input/output error")]
    IO(#[from] std::io::Error),
    #[error("npm install error")]
    NpmInstall(ExitStatus),
    #[error("consttree generation error")]
    ConstTreeGen(ExitStatus),
    #[error("stark info generation error")]
    StarkInfoGen(ExitStatus),
    #[error("C helpers generation error")]
    CHelpersGen(ExitStatus),
    #[error("C helpers compilation error")]
    CHelpersCompile(ExitStatus),
    #[error("proof generation error")]
    ProofGen(ExitStatus),
    #[error("proof verification error")]
    ProofVerify(ExitStatus),
}

fn print_cmd(cmd: &Command) {
    log::info!(
        "→ {} {}",
        cmd.get_program().to_string_lossy(),
        cmd.get_args().map(|arg| arg.to_string_lossy()).format(" ")
    );
}

fn print_and_run(cmd: &mut Command, err: fn(ExitStatus) -> Error) -> Result<(), Error> {
    print_cmd(cmd);
    match cmd.status() {
        Ok(status) => {
            if status.success() {
                Ok(())
            } else {
                Err(err(status))
            }
        }
        Err(e) => Err(Error::IO(e)),
    }
}

fn node_command() -> Command {
    let mut cmd = Command::new("node");
    cmd.arg(format!("--max-old-space-size={MAX_NODE_MEM}"));

    cmd
}

pub fn generate_proof(
    pil_json: &Path,
    starkstruct_json: &Path,
    constants_bin: &Path,
    commits_bin: &Path,
    output_dir: &Path,
) -> Result<OutputFiles, Error> {
    let externals_dir = deps_dir();
    let pil_stark_root = externals_dir.join("pil-stark");
    let pil_stark_src = pil_stark_root.join("src");

    let zkevm_prover_dir = externals_dir.join("zkevm-prover");

    let verification_key_json = output_dir.join("verification_key.json");
    let consttree_bin = output_dir.join("consttree.bin");
    let starkinfo_json = output_dir.join("starkinfo.json");
    let chelpers_bin = output_dir.join("chelpers.bin");
    let chelpers_header_dir = output_dir.join("chelpers");
    let dynamic_chelpers = output_dir.join("dynamic_chelpers.so");

    log::info!("Generating constants merkle tree...");
    {
        let mut cmd = node_command();
        cmd.arg(pil_stark_src.join("main_buildconsttree.js"))
            .arg("-c")
            .arg(constants_bin)
            .arg("-j")
            .arg(pil_json)
            .arg("-s")
            .arg(starkstruct_json)
            .arg("-t")
            .arg(&consttree_bin)
            .arg("-v")
            .arg(&verification_key_json);

        match print_and_run(&mut cmd, Error::ConstTreeGen) {
            Ok(_) => (),
            Err(Error::ConstTreeGen(_)) => {
                log::warn!(
                    "Const tree generation failed, but this might be just missing dependencies."
                );
                log::info!("Trying to install npm dependencies...");
                print_and_run(
                    Command::new("npm")
                        .arg("install")
                        .current_dir(&pil_stark_root),
                    Error::NpmInstall,
                )?;

                log::info!("Retrying constants merkle tree generation...");
                print_and_run(&mut cmd, Error::ConstTreeGen)?;
            }
            Err(e) => return Err(e),
        }
    }

    log::info!("Generating STARK info...");
    print_and_run(
        node_command()
            .arg(pil_stark_src.join("main_genstarkinfo.js"))
            .arg("-j")
            .arg(pil_json)
            .arg("-s")
            .arg(starkstruct_json)
            .arg("-i")
            .arg(&starkinfo_json),
        Error::StarkInfoGen,
    )?;

    log::info!("Generating C helpers...");
    print_and_run(
        node_command()
            .arg(pil_stark_src.join("main_buildchelpers.js"))
            .arg("-s")
            .arg(&starkinfo_json)
            .arg("-c")
            .arg(&chelpers_header_dir)
            .arg("-C")
            .arg("All")
            .arg("-b")
            .arg(&chelpers_bin),
        Error::CHelpersGen,
    )?;

    log::info!("Compiling C helpers into a shared library...");
    print_and_run(
        Command::new("g++")
            .args([
                "-std=c++17",
                "-shared",
                "-fPIC",
                "-fopenmp",
                "-mavx2",
                "-O3",
                "-DNOMINMAX",
                "-o",
            ])
            .arg(&dynamic_chelpers)
            .arg(externals_dir.join("zkevm-prover/test/examples/dynamic_chelpers.cpp"))
            .arg(format!("-I{}", chelpers_header_dir.to_str().unwrap()))
            .args(
                [
                    "src/config",
                    "src/starkpil",
                    "src/utils",
                    "src/goldilocks/src",
                    "src/rapidsnark",
                ]
                .map(|p| format!("-I{}", zkevm_prover_dir.join(p).to_str().unwrap())),
            ),
        Error::CHelpersCompile,
    )?;

    log::info!("Generating proof...");
    let proof_output_dir = output_dir.join("runtime/output");
    fs::create_dir_all(&proof_output_dir)?;
    print_and_run(
        Command::new(zkevm_prover_dir.join("build/zkProverTest"))
            .args(
                [
                    constants_bin,
                    &consttree_bin,
                    &starkinfo_json,
                    commits_bin,
                    &chelpers_bin,
                    &dynamic_chelpers,
                    &verification_key_json,
                ]
                .iter()
                .map(|p| {
                    // Since we run the prover from a different directory, we
                    // have to make all paths absolute.
                    p.canonicalize().unwrap()
                }),
            )
            .current_dir(output_dir),
        Error::ProofGen,
    )?;

    Ok(OutputFiles {
        verification_key_json,
        starkinfo_json,
        proof_json: proof_output_dir.join("jProof.json"),
    })
}

pub fn verify_proof(
    verification_key_json: &Path,
    starkinfo_json: &Path,
    proof_json: &Path,
    publics_json: &Path,
) -> Result<(), Error> {
    let externals_dir = deps_dir();
    let pil_stark_root = externals_dir.join("pil-stark");
    let pil_stark_src = pil_stark_root.join("src");

    log::info!("Verifying proof...");
    let mut cmd = node_command();
    let mut vproc = cmd
        .arg(pil_stark_src.join("main_verifier.js"))
        .arg("-v")
        .arg(verification_key_json)
        .arg("-s")
        .arg(starkinfo_json)
        .arg("-o")
        .arg(proof_json)
        .arg("-b")
        .arg(publics_json)
        .stdout(Stdio::piped())
        .spawn()?;

    let voutput = BufReader::new(vproc.stdout.as_mut().unwrap());

    let mut last_line = String::new();
    for line in voutput.lines() {
        last_line = line.unwrap();
        println!("{}", last_line);
    }

    let status = vproc.wait()?;

    if status.success() && last_line == "Verification Ok!!" {
        Ok(())
    } else {
        Err(Error::ProofVerify(status))
    }
}

#[cfg(test)]
mod tests {
    use std::env::set_current_dir;

    use super::*;
    use test_log::test;

    #[test]
    fn prove_and_verify_absolute() {
        let output_dir = mktemp::Temp::new_dir().unwrap();
        prove_and_verify(&output_dir);
    }

    #[test]
    fn prove_and_verify_relative() {
        let abs_output_dir = mktemp::Temp::new_dir().unwrap();

        // Run from the parent path of the output directory, where output_dir is a relative path.
        set_current_dir(abs_output_dir.as_path().parent().unwrap()).unwrap();
        let rel_output_dir = abs_output_dir.file_name().unwrap();

        prove_and_verify(Path::new(rel_output_dir));
    }

    fn prove_and_verify(output_dir: &Path) {
        let test_data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-data");

        let pil_json = test_data_dir.join("constraints.json");
        let starkstruct_json = test_data_dir.join("starkstruct.json");
        let constants_bin = test_data_dir.join("constants.bin");
        let commits_bin = test_data_dir.join("commits.bin");
        let publics_json = test_data_dir.join("publics.json");

        let output_files = generate_proof(
            &pil_json,
            &starkstruct_json,
            &constants_bin,
            &commits_bin,
            output_dir,
        )
        .expect("proof generation failed");

        assert!(output_files.verification_key_json.exists());
        assert!(output_files.proof_json.exists());

        verify_proof(
            &output_files.verification_key_json,
            &output_files.starkinfo_json,
            &output_files.proof_json,
            &publics_json,
        )
        .expect("proof verification failed");
    }
}
