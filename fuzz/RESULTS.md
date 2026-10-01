# Fuzz qualification

PASS: stable hostile-input fallback, 100,000 inputs per target, 300,000 total; no panics or aborts.

Nightly and cargo-fuzz installation into the user toolchain failed because its directories were read-only. The specified stable fallback was used.

| Target | Inputs | Successful API calls | Rejected inputs | Seconds |
| --- | ---: | ---: | ---: | ---: |
| image_predict | 100000 | 18271 | 81729 | 0.499 |
| video_predict | 100000 | 7035 | 92965 | 102.870 |
| display_parse | 100000 | 1563 | 98437 | 0.534 |

Seed: `0x435656445046555a`.
Source SHA-256: `4412009258328897a228868d1eed64da84c74bb286695f9400f336f67cbcc360`.
Executable SHA-256: `d93f6003140f5080e099497b455172c26969d5d1dc4d9ede8ca4f687ff004063`.

No crashing inputs were found; no crash minimization was required. These are deterministic stress-test results, not coverage-guided libFuzzer results.

Run `python3 fuzz/run_stable.py` to reproduce the fallback and its source/compiler/binary record; see `README.md` for coverage-guided targets.
