# Client Key naming contract

`POST /api/client/keys/{inspect,issue,recover,confirm,applied}` retains its existing authentication, workspace permissions, device/tool scope, fingerprints and issuance request/generation semantics.

The optional integer request field `naming_version` advertises the client's understood naming rule: absent or `0` is the original protocol; `1` understands workspace-aware Token Market names. Other values are rejected before issuance. It is a capability, not a request to edit a name or evidence that adoption has completed.

Every updated response includes integer `naming_version`, reporting the authoritative adoption state of its returned binding: `0` for a historical binding, no current Key, or Miao; `1` for a Token Market Key issued under the new rule. The platform persists it atomically with the Key/configuration/binding transaction in `client_key_bindings.naming_version`. Display-name edits, profile changes, reconnects and client cache contents do not determine adoption. Historical Key proof/adoption remains `0` in this slice.

Every newly issued Token Market Key adopts `1`, including requests from older clients. A personal Bound Workspace uses only the registered product display name. The canonical personal compatibility rule remains `defaultWorkspace == true || type == PERSONAL`. Enterprise names append the authenticated Key owner's primary full phone, then a bound PHONE contact (same feature preferred), then user ID. Only the `+86` component and its following separators are removed. Other international phone strings retain their country code. Names do not identify or merge device scopes. Miao retains `Miao-agent` and adoption `0`.

In the `llm-platform` repository, deploy `docs/sql/temp/20261105_client_key_naming_version.sql` before the updated services. It is idempotent and defaults every existing binding to `0`; the initial creation SQL contains the same column. No historical Key is renamed, reset, disabled or removed by this schema upgrade.

The shared CLI/desktop ConnectionManager sends capability `1` and encrypts the echoed version in its saved issuance receipt (absent legacy receipts deserialize as `0`). Safe credential storage still precedes confirmation, model loading and configuration commit. This slice establishes new scopes and subsequent reuse. Historical scope replacement and stricter mixed-version-server handling follow separately; version alone does not establish a fingerprint match or authorize credential-loss recovery.
