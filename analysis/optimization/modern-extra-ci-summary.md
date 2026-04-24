# Modern Extra CI Summary

## Benchmarks

| media | total | decode | frame | segments |
|---|---:|---:|---:|---:|
| testdata/raw/elephantsdream_teaser.mp4 | 849ms | 448ms | 395ms | 0 |
| testdata/raw/caminandes_gran_dillama.mp4 | 1605ms | 844ms | 750ms | 0 |

## FP Eval (Best Candidate)

Best candidate: `low_threshold`

| media | fp_rate | suspicious | total_non_voice |
|---|---:|---:|---:|
| testdata/raw/elephantsdream_teaser.mp4 | 100.00% | 2 | 2 |
| testdata/raw/caminandes_gran_dillama.mp4 | 14.29% | 1 | 7 |
