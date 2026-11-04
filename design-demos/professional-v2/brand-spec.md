# 品牌资产
现有产品标志：../../src/tauri-ui/assets/token-market.png，已查看。现有界面：../screenshots/integrated-home-1440.png，历史截图；当前依据 src/tauri-ui/index.html、app.css、tools.js、workspace.js 与 window_lifecycle.rs 复核。

本次用户明确要求重设计 Logo。三套候选 assets/mark-1.svg、mark-2.svg、mark-3.svg 是新的矢量设计提案；assets/tray-*.svg 为透明底纯黑版本，后续用于 template image。禁止给状态栏加白色底板。小尺寸需验证像素对齐。

工具 Logo：全部复用当前 src/tauri-ui/assets/tools/ 中的真实资产，不重画。沿用源码 SOURCES.md 的来源记录，逐个嵌入 HTML。

配色：保留原产品蓝色动作语义 #2864df；A 墨色 #242830、背景 #f7f8fa；B 石墨 #121518、动作薄荷 #99e4cc（提案）；C 白色工具库，原品牌蓝作为激活状态。字体：系统无衬线 + 系统等宽，中文 PingFang SC。

主角素材：真实工具图标、状态数据布局和新单色标志，不增加营销照片。数字来自明确标注的原型 fixture。
