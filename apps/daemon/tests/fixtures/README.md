# Loom Contract Fixture Snapshots

These JSON files are the minimum shared-contract snapshots consumed by Loom's
daemon tests. They were imported from the Neuro parent contracts at commit
`be4bbb7b93a2fdc6723af01d066d84824312d76c` so a standalone Loom clone can
validate its public capability and Tea integration envelopes without reading
files outside this repository.

The Tea request's `workspace_root` is normalized to `.` because the absolute
parent checkout path is not part of the contract. Canonical cross-project
schemas remain owned by the integrating suite; these files are Loom test
fixtures, not a second schema source.
