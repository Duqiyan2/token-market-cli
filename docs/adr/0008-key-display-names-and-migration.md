---
status: accepted
---

# Replace historical tool Keys once when adopting user-identifying names

Keys for the same Tool in an Enterprise Workspace currently have
indistinguishable names. In an Enterprise Workspace, use the Tool name plus the
owner's full phone number, removing China's `+86` prefix, retaining other country
codes and falling back to user ID when no phone exists. In a Personal Workspace,
use only the Tool name, without a phone number or user ID. Classify the Key's
Bound Workspace using the platform's existing workspace semantics; identical
names across a user's devices are acceptable.

Adopting the rule creates a new Key once per existing connection scope instead
of renaming historical Keys. Keep historical Keys unchanged and reuse the new
Key afterward. This deliberately adds a one-time issuance and migration state
in exchange for preserving history; existing credential-loss recovery cannot
stand in for migration because it may reset or revoke the historical Key.
Track adoption explicitly rather than interpreting editable names as identity
or migration state. See the [feature specification](../client-key-naming.md).
