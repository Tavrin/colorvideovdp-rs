Historical pre-hardening measurements; Rust source and executable identities
were not captured. These numbers do not qualify the current implementation.

Single thread:

| 1080p workload | Rust (ms) | PyTorch CPU (ms) | PyTorch / Rust |
| --- | ---: | ---: | ---: |
| Image, noise | 1149.94 | 4513.44 | 3.9× |
| Video, 8 frames at 30 fps, flicker | 11494.68 | 32677.17 | 2.8× |

Multi-threaded (16 Rayon workers; 16 PyTorch intra-op threads):

| 1080p workload | Rust (ms) | PyTorch CPU (ms) |
| --- | ---: | ---: |
| Image, noise | 194.53 | 809.62 |
| Video, 8 frames at 30 fps, flicker | 2086.01 | 9201.24 |
