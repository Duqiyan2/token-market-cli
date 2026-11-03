# Separate client login from tool connections

A Client Session lets the user manage local tools through either the CLI or
desktop. Each Tool Connection retains its own account, Bound Workspace, configured
models and restoration records. Logout and workspace switching preserve existing
connections, avoiding unexpected configuration or billing changes.

Connect and Reconnect configure tools without calling a model. Connection Test
is a separate, charge-confirmed action using one configured model and the saved
connection. A passing check proves one complete text response, rather than all
Agent capabilities. Disconnect restores managed configuration fields and preserves
later manual edits through explicit conflict choices.
