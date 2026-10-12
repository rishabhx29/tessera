# Week-1 spike — measure before building

Answers one question with real files on real hardware: **do the product's
chosen models and store meet the latency targets in
[`docs/product-definition.md`](../docs/product-definition.md) §6?**

The spike is deliberately Python even though the engine is Rust (ADR-0001):
its job is model *latency*, and the ONNX graphs are identical to what the Rust
`ort` bindings will load. Model time dominates wrapper overhead, so these
numbers transfer; everything about the delivery (installers, process model)
stays Rust.

## Run

```powershell
pip install -r spike/requirements.txt
python spike/spike.py --images 120 --docs 40      # smoke run
python spike/spike.py --images 3000 --docs 500    # real run
```

Flags: `--images N`, `--docs M` (corpus size, newest-first), `--include-cloud`
(also index OneDrive online-only placeholders — they're skipped by default,
which is itself a rule the engine enforces), `--query-runs`, `--scales`.

## What it measures

1. **Per-stage cost** — decode, OCR, CLIP embed; extract, chunk, MiniLM embed.
2. **End-to-end** — one image / one document from file to searchable.
3. **Fused query latency** — lexical (FTS5 trigram), image arm (SigLIP
   text→vector), text arm (MiniLM), and RRF fusion, at the real corpus size.
4. **Scale estimates** — same store at 1k/10k/50k rows via replication. These
   are labelled estimates everywhere; only the real-corpus rows are
   measurements.
5. **Cross-modal sanity** — for images with OCR text, rank of the correct
   image when querying with that text. A near-random mean rank means the
   embedding space is wrong, which fails the spike regardless of speed.

## Pass / fail

| Target | Source |
|---|---|
| image end-to-end < 1500 ms | product-definition §6 |
| doc end-to-end < 300 ms | product-definition §6 |
| fused query < 100 ms at real N | product-definition §6 |
| cross-modal top-1 rate clearly above chance | this spike's own gate |

OCR at >1 s/image p95 is the named failure mode (ADR-0004): Windows.Media.Ocr
becomes the fallback and the engine gains a platform branch.

## Outputs

- `spike/results/results.json` — full detail (timings, environment, model hashes)
- `spike/results/results.md` — the table to paste into `docs/performance.md`
- `spike/model-lock.json` — SHA-256 of every model file; later runs verify
  against it. First run is trust-on-first-use; the shipping product pins
  known-good upstream checksums instead (ADR-0004).

## Privacy

Nothing leaves the machine. `results.json` contains text extracted from your
own files — keep it out of git (`spike/results/` is not committed; the tables
in `docs/performance.md` are what ship).
