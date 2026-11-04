# 已核实产品事实
- 本仓库 README.zh-CN.md 列出 14 种工具；实际桌面支持工具安装、连接恢复、模型测试、空间切换与后台常驻。版本不在界面中新增主张。
- src-tauri/tauri.conf.json：当前窗口 900×760。
- src-tauri/src/window_lifecycle.rs:143–169：当前托盘复用 default_window_icon；注释明确它不是 macOS 单色 template image。白底来自当前品牌位图；正式替换需要专用透明模板图标。
- Linear 官方显示选项支持列表/看板、分组和排序：https://linear.app/docs/display-options（2026-11-04 查阅）。
- Warp 官方窗口文档列出分栏：https://docs.warp.dev/terminal/windows（2026-11-04 查阅）。参考只用于交互结构；不声称复刻某版本。
- ref_board.py 因本机 Python 未安装 Playwright 无法运行，改用已安装的 Node Playwright 截图；若联网截图不可用，在 directions.md 标注。
