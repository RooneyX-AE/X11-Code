# X11 Code Swarm Conflict Contract

X11 treats parallel file edits as safe only when their changed line ranges do not overlap.

A change is represented by:

- `agent_id`
- `path`
- `start_line`
- `end_line`

The conflict analyzer groups changes by path and compares every pair of ranges.

Two changes overlap when:

```text
A.start <= B.end && B.start <= A.end
```

Non-overlapping changes may proceed to an automatic merge stage. Overlapping changes must not be silently merged. They become an explicit resolver task with the involved agent IDs and file path as evidence.

The resolver must create a checkpoint before modifying the workspace and the final merged state must pass the normal verification plan before the swarm can be accepted.

This contract is intentionally deterministic. Model-based conflict resolution may be layered on top of it, but it cannot bypass the overlap decision or verification gate.
