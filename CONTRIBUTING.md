# Contributing

Bug reports and pull requests are welcome. For security issues, see
[SECURITY.md](SECURITY.md).

## Build and test

```sh
cargo build
cargo test --all-features
cargo test --no-default-features
cargo build --target wasm32-unknown-unknown --no-default-features
```

The minimum supported Rust version is 1.85.

## Style

CI runs these, and they must pass:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

New code must not use `unsafe`, `unwrap`, `expect` or `panic!` outside tests;
the crate's lints reject them.

## Parity

The crate must produce the same results as the PyTorch reference at revision
`2a268bc`. Any change to numerical code (colour conversion, display model,
pyramid, CSF, masking, temporal filters, pooling) must keep the 150-case
corpus and the 500-case sweep passing without loosening the tolerances. Run
them before opening a pull request:

```sh
git clone https://github.com/gfxdisp/ColorVideoVDP
git -C ColorVideoVDP checkout 2a268bc
export CVVDP_REFERENCE="$PWD/ColorVideoVDP" CVVDP_PYTHON=path/to/python

parity/run.sh     # 150-case corpus
parity/sweep.sh   # 500-case randomized sweep
```

[`parity/README.md`](parity/README.md) lists the Python requirements and
describes the harness. If a change affects performance, include the output of
`parity/benchmark.sh` in the pull request.

## Sign-off

Commits must be signed off under the Developer Certificate of Origin
(<https://developercertificate.org/>): use `git commit -s`, which adds a
`Signed-off-by:` line with your name and email.

## Licence

Contributions are licensed under the MIT licence in [LICENSE](LICENSE).
