# Qualified corpus parity

Rust source/configuration SHA-256: `9c412490fde1c6881f0f5a3d284b3f150afdfc2c01b60d1aabf6494c00855443`.
Single-thread executable SHA-256: `221505218f84d539ddf8e9d34a96bc2c9a90ed66ca050b379296c1e7c19d4b17`.
Parallel executable SHA-256: `848cc8da4ac20b7e35f9e216e23b0a5d1b11a946d8f50d18736e01054b8038cd`.

150 cases pass with unchanged original maxima. Features and spatial frequencies
also pass their gates, and every serialized prediction byte matches between
the two execution modes. `HARDENING_RESULTS.json` binds these results and the
500-case sweep to execution-time source, compiler, configuration, input and
output identities.

| Corpus | Cases | Max absolute JOD error | Max raw map error | Max map error before f16 |
| --- | ---: | ---: | ---: | ---: |
| generated images | 42 | 0.00000095 | 0.00024408 | 0.00001842 |
| generated videos | 42 | 0.00000286 | 0.00097513 | 0.00001389 |
| other displays | 19 | 0.00000095 | 0.00010151 | 0.00000030 |
| colour spaces | 22 | 0.00000191 | 0.00023419 | 0.00012809 |
| temporal edges | 8 | 0.00000095 | 0.00024098 | 0.00000030 |
| spatial edges | 8 | 0.00000095 | 0.00011790 | 0.00000024 |
| grayscale | 3 | 0.00000000 | 0.00010574 | 0.00000089 |
| reference media | 6 | 0.00000095 | 0.00017059 | 0.00000089 |
| **Total** | **150** | **0.00000286** | **0.00097513** | **0.00012809** |
