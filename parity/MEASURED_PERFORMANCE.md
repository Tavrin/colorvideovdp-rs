Single thread:

| 1080p workload | Rust (ms) | PyTorch CPU (ms) | PyTorch / Rust |
| --- | ---: | ---: | ---: |
| Image, noise | 737.93 | 2295.32 | 3.1× |
| Video, 8 frames at 30 fps, flicker | 9241.50 | 27338.51 | 3.0× |

Multi-threaded (16 Rayon workers; 16 PyTorch intra-op threads):

| 1080p workload | Rust (ms) | PyTorch CPU (ms) |
| --- | ---: | ---: |
| Image, noise | 200.88 | 665.19 |
| Video, 8 frames at 30 fps, flicker | 1990.17 | 8928.44 |
