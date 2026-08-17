# Conflict resolution contract

X11 separates deterministic conflict detection from model-assisted resolution.

1. `ConflictResolver::analyze` identifies overlapping file/line ranges.
2. Only `ResolveRequired` conflicts may create a `ResolutionProposal`.
3. A proposal is valid only when its path, line range, and source agents belong to the reported conflict.
4. The resolver model receives a narrow `ConflictHunk`, not the full swarm context.
5. Applying a proposal is a separate mutation step and must be followed by verification.
6. Rejected proposals never bypass the normal permission and checkpoint boundaries.

This keeps model-assisted merging bounded and auditable while allowing safe non-overlapping edits to proceed without a model merge step.
