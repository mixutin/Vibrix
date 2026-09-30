# Upstream port patch tracking

Every future Vibrix port that carries local patches registers a
`ports/<name>/tracking.toml` record. The record pins the upstream GitHub
repository/ref and exact 40-hex commit, and lists every local patch with its
SHA-256 digest.

`tools/check-port-upstreams.py` performs two layers of checking:

1. deterministic repository validation verifies schema, patch existence,
   duplicate entries and exact patch digests;
2. optional `--network` mode resolves each tracked GitHub ref and fails when
   it no longer matches the pinned commit.

The normal repository workflow runs the deterministic checks. A scheduled and
manual workflow runs network mode so upstream movement becomes visible without
making ordinary builds depend on third-party availability.

An empty `ports/` tree is valid before representative ports land. Once a port
tracking record exists, malformed metadata, patch drift or upstream ref movement
fails the corresponding check instead of being silently ignored.

This mechanism tracks upstream movement; it does not automatically rebase,
rewrite, download, or apply third-party patches.
