# Graphical tool installation

Installation, source/fixture checks, native compilation and actual Agent runtime
checks are separate verification steps.

## WorkBuddy

The installer catalogue selects the official WorkBuddy 5.9.3 distribution
(versioned build `5.9.3.42398134-d4692421`): a macOS Apple Silicon/Intel DMG or
Windows x64 user installer. The client downloads and opens the package. Complete
the drag to Applications or native wizard yourself. Downloading, opening or
closing that installer does not establish installation success.

The client waits for actual application evidence and compatible version metadata,
then refreshes installation and connection status. It preserves existing compatible
installations and does not automatically replace or upgrade an unverified version.
Linux environments without a verified general-purpose package show the reason;
the official UOS/Kylin application-store guidance remains an alternative.

Source: [official WorkBuddy archive](https://www.codebuddy.cn/docs/workbuddy/Download-History).

## DeepSeek Harness

The user approved **exactly `0.2.1-rc.5` for Harness** as an exception to the stable
release policy. It is a release candidate, not a stable release. Other tools keep
their stable-release rule.

- macOS Apple Silicon: download the fixed official ZIP, verify the official
  feed's SHA512, extract to staging and show its folder in Finder. Move the app
  into Applications. The client does not launch the app to initialize a profile
  or log in.
- Windows x64: download the fixed official EXE, verify its published SHA512 and
  open the native installation wizard.
- Linux x64/arm64 and macOS Intel: install the fixed official npm CLI into a
  private prefix. Reuse a usable Node/npm runtime, or prepare a private official
  Node 24.22.1 runtime with its published SHA256. The persistent `dsh` wrapper
  keeps explicit arguments and defaults a bare invocation to the `web` profile.

Desktop application evidence and `dsh` command registration are independent. A
desktop install does not need to register a global command; use Harness's own
command-management interface when wanted. Version verification retains the full
RC identifier. For the managed standalone CLI without a GUI, connection settings
use Harness's home patch rather than an Electron-only `desktop` profile. Explicit
`DSH_HOME` and profile choices retain their established meanings.

Sources: [Harness command management](https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/desktop/README.md),
[official CLI package](https://www.npmjs.com/package/@deepseek-ai/dsh),
[Node checksums](https://nodejs.org/dist/v24.22.1/SHASUMS256.txt).

## Waiting, cancellation and connection

The installation dialog shows progress and asks for required native steps.
Closing a waiting dialog continues monitoring. **停止等待 / Stop Waiting** stops
the client's monitoring; cancel a native installer in its own interface. The
client does not terminate that installer or remove completed dependencies/files.
A late installation is discovered by a fresh inventory read or retry.

Installation does not configure a Tool Connection, issue a Key, log in to the
Agent or make a model request. After successful detection, choose Connect or
Reconnect separately. Existing connections and user configuration remain intact.
Actual app startup, streaming and tool calling require separate release acceptance.
