# protocols/

Shared rules every agent follows. Protocols are loaded by agents at runtime
(JIT) — the orchestrator and individual agents reference these documents by
filename when they need the contract for a particular operation.

## Why protocols?

Agents are role definitions; protocols are the contracts that bind them. A
protocol like `progress-schema.md` is read by every agent that touches
`progress.json`, ensuring they all agree on the shape and semantics. Putting
this content in protocols (rather than duplicating it in each agent body)
means a contract change updates in one place and propagates.

## The protocol set

| Protocol               | Defines                                                       |
|------------------------|---------------------------------------------------------------|
| `progress-schema.md`   | The progress.json contract — schema, fields, lifecycle        |
| `spec-format.md`       | How to write a spec — required sections, naming, ID assignment|
| `gate-checks.md`       | Verification strategy and retry ladder for phase execution    |
| `self-review.md`       | Self-review procedure used at Gate 3 by code-reviewer         |
| `commit-message.md`    | Commit message conventions for /commit and phase-implementer  |
| `rationale-format.md`  | Format of the rationale.md companion produced by spec-narrator|
| `profile-schema.md`    | Developer profile schema and granularity vector               |
| `skip-log.md`          | Skip log format, weighting, threshold, and sliding window     |

## New in PILOT (not in RAID)

The bottom three protocols are PILOT-specific:

- `rationale-format.md` — defines what the spec-narrator produces
- `profile-schema.md` — defines the developer profile structure
- `skip-log.md` — defines the calibration loop's data contract

The first five are direct ports/adaptations of RAID's protocol set.
