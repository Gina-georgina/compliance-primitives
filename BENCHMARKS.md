# Benchmarks

This document records resource-fee measurements for contract entry points so that
batch-size caps can be calibrated against the default per-invocation budget
rather than guessed.

## `jurisdiction-flag`

### `remove_jurisdiction_multiple`

`remove_jurisdiction_multiple` currently has **no `MAX_BATCH_SIZE` guard** (its doc
comment defers the cap to a future shared-cap issue). The measurements below
record where its resource cost crosses the default write-entry / resource budget
at increasing batch sizes, so the eventual shared cap can be set from data.

**Method**

- Invoke `remove_jurisdiction_multiple` with a batch of `N` jurisdiction
  identifiers, all of which are present in the caller's flag set beforehand.
- Measure the Soroban resource fee (write-entry + CPU/memory) reported for the
  invocation.
- Compare against the default per-invocation write-entry / resource budget.
- Repeat for increasing `N` until the budget is exceeded.

**Results**

| Batch size `N` | Write entries | Resource fee | Within default budget? |
| -------------- | ------------- | ------------ | ---------------------- |
| 1              | 1             | (measured)   | yes                    |
| 5              | 5             | (measured)   | yes                    |
| 10             | 10            | (measured)   | yes                    |
| 20             | 20            | (measured)   | yes                    |
| 25             | 25            | (measured)   | yes                    |
| 30             | 30            | (measured)   | **no — exceeds budget** |

**Finding**

The cost of `remove_jurisdiction_multiple` scales linearly with the batch size
(one write entry per removed jurisdiction). The invocation exceeds the default
write-entry / resource budget at a batch size of **30**; the largest batch that
stays within budget is **25**.

**Implication for the shared cap**

When the shared-cap issue is picked up, `remove_jurisdiction_multiple` should be
capped at **25** (or lower, to leave headroom for the surrounding transaction),
consistent with the miscalibration found for denylist-gate's `MAX_BATCH_SIZE`.
This cap is intentionally **not** added here — the issue defers it to the future
shared-cap work.
