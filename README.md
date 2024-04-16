This crate uses the external submodules
[pil-stark](git@github.com:powdr-labs/pil-stark.git) and
[zkevm-prover](https://github.com/powdr-labs/zkevm-prover) to generate EStark ZK
proofs from a Rust friendly interface.

At runtime, it needs to find the contents of cargo's `OUT_DIR` (which is
populated by `build.rs` at build time), otherwise execution will panic. This
means that, if used as a cargo dependency from the same machine it is built,
e.g. with `cargo run` or `cargo test`, it will work out of the box. But if the
binaries are executed from another machine, e.g. from a `nextest` archive, it
will fail unless the original `OUT_DIR` contents are manually provided.

The library will first search for the dependencies in the path given by
environment variable `PIL_STARK_PROVER_DEPS`, and if not set, it fallbacks to
the compile-time hardcoded path set by cargo's `OUT_DIR`, which is typically
`target/<profile>/build/pil-stark-prover-<hash>/out`.
