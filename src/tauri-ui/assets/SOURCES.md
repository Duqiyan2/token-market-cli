# UI asset sources

Token Market mark: copied from src-tauri/icons/128x128.png.

Official public site marks retrieved 2026-10-29 during the approved UI design:

- claude: https://assets.claude.com/95a868946ac8a31e5ff832e2899f294aa368b836.png?w=32&h=32
- codex: https://openai.com/favicon.ico
- workbuddy: https://download.codebuddy.ai/web/workbuddy/170b416d2eccaaf590ea40cd918d5a6b4f07178f/assets/logo.svg
- deepseek: https://www.deepseek.com/favicon.ico
- openclaw: https://openclaw.ai/favicon.svg
- opencode: https://opencode.ai/favicon-96x96-v3.png

DuMate: extracted from `Contents/Resources/icon.icns` in the official 1.0.89
macOS package on 2026-11-03; resized to 128 px.
https://dumate-app-public.cdn.bcebos.com/release-info/artifacts/Dumate-mac-arm64-latest.dmg

2026-11-04: User selected professional-v2 direction A for both UI and Logo.
`token-market.svg` is the approved transparent monochrome T mark. `token-market.png`
and native platform icons are generated from `src-tauri/icons/app-icon.svg` using
Tauri CLI icon. `tray-template.svg` is the standalone transparent menu bar mask;
`tray-template.png` and its 36×36 RGBA export are rasterized from that SVG.
The macOS tray uses the RGBA mask with `icon_as_template(true)`, not the Dock icon.

2026-11-04 refinement: Dock tile updated from blue to neutral graphite (#242830)
following user feedback; approved A glyph and transparent tray mask unchanged.
