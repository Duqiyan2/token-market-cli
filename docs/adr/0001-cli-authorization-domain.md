# Browser authorization for client login

The CLI and desktop share a Client Session. Login opens browser authorization;
the client validates the returned state and exchanges the one-time result using
PKCE. Browser callbacks contain no reusable session credential or Agent API Key.
On a machine without a browser, the CLI displays a link that can be opened on
another device.

Login establishes management access only. Tool Connection creation is a separate
explicit action, so signing in never edits an Agent's configuration or sends a
model request. See [shared credential handling](0004-shared-client-key-lifecycle.md).
