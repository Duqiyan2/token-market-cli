# Shared local credentials for CLI and desktop

The CLI and desktop manage the same local Tool Connections. For the same account,
workspace, device, tool and configured service address, both reuse a securely
saved Agent API Key. Keeping one shared local record avoids conflicting tool
configuration and restoration histories.

## Client behavior

- Connect and Reconnect validate a saved credential before reuse. A deleted or
  reset credential can be replaced; a disabled or expired credential requires
  user action. Network, permission, balance and model errors never imply that a
  credential is missing.
- Save newly received credentials in encrypted local storage before loading the
  model catalogue or publishing tool configuration. An interrupted operation
  resumes from its saved state instead of silently requesting another credential.
- If the saved credential is missing, inspect only the relevant tool's existing
  configuration for a matching local copy. A locked or unreadable store is an
  error, rather than evidence of loss.
- Recovery after all local copies are lost requires explicit confirmation.
  Publish the replacement configuration before finalizing recovery. Persist
  unfinished work so a later explicit operation can resume it.
- Disconnect restores managed fields while retaining the saved credential.
  Logout ends shared management login and preserves tool configuration.
- Cross-process locks serialize local operations. Reject stale confirmation
  contexts when the shared login or current workspace changes.

## Local storage and configuration

Credentials, connection records and restoration backups are encrypted. macOS and
Windows use native credential stores. Linux prefers Secret Service and can use a
passphrase-encrypted local vault when it is unavailable. Noninteractive use without
an available unlock source fails instead of writing plaintext.

Maintain one active local configuration per tool. Preserve unknown settings and
later manual changes; conflicts require an explicit choice. Save original values
for each managed field rather than overwriting a whole file with an old backup.
Public status summaries never contain credentials.

## Verification

Use temporary directories, local HTTP fixtures and fake private stores for
credential reuse, interrupted operations, explicit recovery, configuration
conflicts and scope isolation. OS credential-store checks and installed-client
checks remain separate from simulated tests. Paid model checks require a separate
explicit action.
