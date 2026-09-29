# Update channel policy

Vibrix exposes three explicit update channels: `stable`, `beta` and
`nightly`. The update-policy state machine owns the selected channel and rejects
a staged release whose channel does not match the current selection.

A fresh policy inherits the channel of its known-good release. The current
compatibility constructor creates stable releases, while
`Release::new_in_channel` creates an explicitly labelled release. Channel names
are parsed strictly and unknown/case-variant names fail closed.

Changing channel is allowed only when no trial update is pending. This prevents
an in-flight trial from being silently reinterpreted under another channel.

This is control-plane policy only. It does not claim signed repositories,
persistent channel configuration, artifact download, USB writes, or automatic
update execution.
