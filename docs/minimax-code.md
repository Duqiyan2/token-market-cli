# MiniMax Code

The unreleased source build supports MiniMax Code as `minimax-code`, command
`mcode`. Install uses the official **0.7.2 GitHub release package**, paired with
source revision `c6c7f28e73921d7295b9c060dee478ced75cf438` and SHA256
`aa055b696945286cbab62f8b748835de1428e4d08a598bbf5de4c8a5de71f4e4`.
The npm latest package (0.7.4 when checked) is a different artifact; matching
product/version names alone do not establish schema or build provenance. Other
installed versions are detected and retained; this installer does not claim a
compatible 0.7.x range or upgrade them automatically.

The package supports Node 22.19+ on 22.x, 24.2+ on 24.x, 25 or 26. The installer
reuses a compatible Node/npm pair or prepares official Node **24.22.1**, verified
by its platform SHA256, in a private tool directory. It retains the complete
package and runtime, enables optional dependencies and the required SQLite
installation script, and leaves persistent PATH and existing installations alone.
macOS, glibc Linux and Windows x64/arm64 have installation plans. Alpine/musl
and other architectures have no supported automatic plan. If SQLite needs a
source build instead of a prebuilt binding, the installation output identifies
the missing Python/C++ system prerequisites; prepared runtime files remain for retry.

Configuration belongs to the **data directory**, separate from
`MCODE_INSTALL_DIR` or `~/.minimax-code` (upstream installation directories):

- A nonempty `MINIMAX_DATA_DIR` wins over `MAVIS_DATA_DIR`.
- Otherwise the default is `~/.minimax/config.yaml`. A retained legacy `.mavis`
  directory is used without migrating or merging it when the new root is absent.
- For an existing `mcode --profile work` workflow, export `MINIMAX_DATA_DIR` to
  `~/.minimax-work` in the Token Market client environment before connecting.
  MiniMax's opt-in internal `__MAVIS_RUNTIME_PROFILE` environment is also respected
  only when `__MAVIS_ALLOW_LEGACY_RUNTIME_ENV=1`; no public `MINIMAX_PROFILE` is invented.
- The client launch binds the selected data root for the child process. Ad hoc
  upstream `--data-dir`/`--profile` launches can select a different configuration;
  changing the active root requires disconnecting its retained connection first.

Connect writes `custom_provider.tokenmarket` with explicit
`api: openai-completions`, `options.authMode: api-key`, gateway `/v1` URL and the
scoped saved key. It imports the complete Key-visible text-generation catalogue
as model maps. Available context/output metadata uses the tagged `limit.context`
and `limit.output` fields. Each leaf has its own restoration receipt; unrelated
models, nested unknown values, `defaultModel` and `defaultModelVariant` survive.
`minimax_api`, the managed `provider` snapshot and login state remain untouched.
Select a model inside MiniMax Code's `/model` after connecting.

Existing OAuth/managed-login, disabled providers, competing authorization headers
or independent model credentials produce an actionable refusal. Matching-gateway
literal key recovery never evaluates environment or command syntax. Connect and
Install do not invoke `provider --use`, onboarding, a model or a default change.
Reconnect reuses the saved key; Disconnect restores owned fields and preserves
later user additions. Test requires charge confirmation, checks one selected
model's non-streaming response, and leaves configuration unchanged.

These are source checks and isolated public-manager/browser fixtures. The official
release reports Linux/macOS package and offline BYOK checks; Windows package and
live-service acceptance were not run upstream. Local native installation on all
platforms and paid Agent streaming/tool-call acceptance remain release checks.

Official sources checked 2026-11-02:

- [0.7.2 release, package hash and source revision](https://github.com/MiniMax-AI/minimax-code/releases/tag/v0.7.2)
- [Tagged installation and artifact provenance](https://github.com/MiniMax-AI/minimax-code/blob/v0.7.2/docs/installation.md)
- [Tagged custom provider behavior](https://github.com/MiniMax-AI/minimax-code/blob/v0.7.2/docs/examples.md)
- [Tagged data/profile and model/provider schema](https://github.com/MiniMax-AI/minimax-code/blob/v0.7.2/packages/config/src/config.ts)
- [Tagged authentication mode](https://github.com/MiniMax-AI/minimax-code/blob/v0.7.2/packages/config/src/provider-auth-mode.ts)
- [Official Node checksums](https://nodejs.org/dist/v24.22.1/SHASUMS256.txt)
