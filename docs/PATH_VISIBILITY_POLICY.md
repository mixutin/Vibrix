# Path visibility/access allow-list

Vibrix M23 uses an independently designed monotonic path-authority primitive.
A process starts unrestricted. Once restricted, it owns at most eight validated
absolute path prefixes, each carrying read and/or write authority.

Matching is component-aware: a rule for `/tmp` covers `/tmp` and
`/tmp/file`, but never `/tmp2`. Later restrictions may remove prefixes,
narrow a prefix to a descendant, or remove rights. They cannot add a new
uncovered prefix or restore a removed right. Children inherit the parent's
current policy unchanged.

The current kernel API is a policy primitive attached to process identity. It
does not yet wire pathname syscalls through the policy, expose a userspace ABI,
provide mount namespaces, or claim a complete sandbox. Those remain separate
enforcement steps.
