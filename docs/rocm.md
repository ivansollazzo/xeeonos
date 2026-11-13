# ROCm (AMD GPU computing)

Two optional system extensions (see [extensions.md](extensions.md)), from
the same Arch snapshot as the system:

| Extension | What | For |
| --- | --- | --- |
| `rocm` | HIP and OpenCL runtimes, `rocminfo`, `rocm-smi` | running programs that use the GPU |
| `rocm-sdk` | all of `rocm`, plus `hipcc`, the HIP and OpenCL SDKs, the math libraries (rocBLAS, rocFFT, rocRAND, rocSPARSE, rocSOLVER, hipBLAS, ...) and their headers; about 12 GB to download, keep that much free on the disk | building them |

Turn on one of the two:

```
sudo xctl ext enable rocm-sdk
systemctl reboot
rocminfo                # the GPU is listed as an agent (gfx...)
hipcc --version
```

ROCm lives in `/opt/rocm` as usual (a link into the extension). To use the
GPU, a user must be in the groups `render` and `video`:

```
sudo usermod -aG render,video <user>
```

then log in again. Supported cards are the ones ROCm supports (Radeon RX
6000/7000/9000, Radeon Pro, Instinct; some older cards with
`HSA_OVERRIDE_GFX_VERSION`).

## Tests

`tools/ci/rocm.sh` turns `rocm` on in a virtual machine (no AMD GPU there):
`/opt/rocm` is in place, the libraries are found, `rocminfo` and `rocm-smi`
are there. `rocm-sdk` (about 12 GB) is too large to download inside the
virtual machine in reasonable time: the test mounts it and checks `hipcc`,
the math libraries and the headers. What runs on a real GPU is checked by
hand.
