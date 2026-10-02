Single thread:

| 1080p workload | Rust (ms) | PyTorch CPU (ms) | PyTorch / Rust |
| --- | ---: | ---: | ---: |
| Image, noise | 944.35 | 5414.94 | 5.7× |
| Video, 8 frames at 30 fps, flicker | 11245.95 | 32015.98 | 2.8× |

Multi-threaded (16 Rayon workers; 16 PyTorch intra-op threads):

| 1080p workload | Rust (ms) | PyTorch CPU (ms) |
| --- | ---: | ---: |
| Image, noise | 336.02 | 851.73 |
| Video, 8 frames at 30 fps, flicker | 3411.55 | 9510.78 |
