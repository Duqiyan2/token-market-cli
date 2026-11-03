# Explicit workspace selection and OpenClaw scope

The client displays its Current Workspace before establishing a Tool Connection.
A connection retains its Bound Workspace when the current selection changes.
Changing an existing connection's account or billing workspace requires explicit
Reconnect confirmation; workspace selection alone never edits tool configuration.

OpenClaw is a supported Tool; qclaw is a separate product and is not detected or
configured. OpenClaw configuration discovery honors `OPENCLAW_CONFIG_PATH` and
uses `~/.openclaw/openclaw.json` by default. Treating another product's directory
as an OpenClaw installation could modify unrelated user settings.
