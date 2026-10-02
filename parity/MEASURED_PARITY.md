# Qualified corpus parity

Rust source/configuration SHA-256: `da57d5b48dc566fd6eda45833634a520de83b94208d854386453621b3a5e81b7`.
Single-thread executable SHA-256: `d3ca16a4943be331471bbdb7d870e0bd3dd5417a16824daeec040f47ac9afd4e`.
Parallel executable SHA-256: `f58d0d0568d20d4c134eff9acd5eb47343d3472be016f3b7e843adc851609bb4`.

150 cases pass with unchanged original maxima. Features and spatial frequencies
also pass their gates, and every serialized prediction byte matches between
the two execution modes. `MEASURED_RESULTS.json` binds these results to
execution-time source, compiler, configuration, input and output identities.
The separate 500-case sweep is summarized in `results/sweep.md`.

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
