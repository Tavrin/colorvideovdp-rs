# Fuzzing

Three targets exercise the public API with malformed shapes, extreme byte and
floating-point values, colour encodings, frame rates, and display JSON. Valid
routes also reach image/video processing, including symmetric padding at the
16384 fps endpoint. The parser target tries arbitrary text and structured
records with hostile float bits, plus geometry and photometry constructors.
The targets share their adapters with the deterministic stable runner.

With nightly and cargo-fuzz installed, run from the repository root:

```sh
cargo +nightly fuzz run image_predict --features fuzzing -- -max_total_time=300
cargo +nightly fuzz run video_predict --features fuzzing -- -max_total_time=300
cargo +nightly fuzz run display_parse --features fuzzing -- -max_total_time=300
```

If nightly installation is unavailable, use the stable fallback:

```sh
cargo run --release --manifest-path fuzz/Cargo.toml --bin hostile -- 100000
```

`python3 fuzz/run_stable.py` runs the same fallback and captures source, compiler
and binary SHA-256 identities in `MEASURED_RESULTS.json`.

This executes 100,000 deterministic hostile inputs **per target**, seeded with
`0x435656445046555a`; each target runs directly, so a panic or abort fails the
process. It includes valid predictions as well as rejected inputs. The fallback
is a stress test, not coverage-guided fuzzing. Preserve and minimize any crashing
input with `cargo fuzz tmin TARGET ARTIFACT`; add a public-API regression test
before accepting a fix. Generated corpora and crash artifacts are excluded.

See `RESULTS.md` for the completed qualification run.
