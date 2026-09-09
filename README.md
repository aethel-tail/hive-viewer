<div align="center">
  <img src="src-tauri/icons/icon.png" width="112" alt="Hive Viewer" />
  <h1>Hive Viewer · 蜂巢看图</h1>
  <p>Windows 上轻量顺滑的桌面看图工具 —— Tauri 2 + Vue 3 + TypeScript</p>
</div>

Hive Viewer（蜂巢看图）是一个面向 Windows 的极简桌面图片浏览器。Rust 后端负责文件系统扫描、EXIF 解析与格式转换，Vue 3 前端负责交互；图片通过 Tauri 的 asset 协议直接从磁盘流式加载（不做 base64 中转），大图冷启动与快速翻页都很流畅。

## 功能

**浏览**

- 支持 `jpg` `jpeg` `png` `gif` `webp` `bmp` `avif`，目录内按自然顺序排序（`1, 2, 10` 而不是 `1, 10, 2`）
- 打开方式：按钮选择文件 / 文件夹、把文件拖进窗口、从资源管理器「打开方式」打开、命令行传入路径
- 双缓冲渲染：新图解码完成后再切换，上一张保持可见，翻页不闪黑
- 缩放方式：适合窗口 / 适合宽度 / 原始大小 / 智能双页预览（左→右、右→左）；范围 25%–3200%，`Ctrl + 滚轮`缩放，滚轮翻页
- 放大后按住鼠标左键拖动即可平移画面，边界自动吸附，图片小于窗口时自动居中
- 显示旋转（只影响显示，不修改原文件）
- 删除文件走**回收站**，可选二次确认
- 到首/末张时可选择：循环、停住并提示、弹窗询问

**幻灯片**

- 间隔 1–90 秒；顺序循环或随机播放
- 过渡效果：无 / 翻转 / 淡入 / 滑动

**EXIF**

- 按标签展示 EXIF 信息，MakerNote 原始数据可一键复制

**格式转换**

- 输出格式：AVIF / WebP / JPEG / PNG / BMP
- 旋转（按 EXIF 自动 / 90° / 180°）与缩放（contain / fit-width / pad 留白 / crop 裁剪 / stretch 拉伸）
- 按格式提供质量或无损选项；重名不覆盖原文件，自动追加 `_1`、`_2`
- 扩展名与实际格式不符的图片（例如 PNG 改名成 `.jpg`）也能正常转换：解码按文件内容识别格式，不依赖扩展名
- AVIF 可浏览、可作为转换的输入与输出：解码走纯 Rust 的 dav1d（re_rav1d），不需要 meson/ninja/NASM 工具链；容器里的 irot/imir 方向会自动校正。仍不支持 grid（分块拼接）与动画（avis）AVIF，这类文件会返回明确错误
- 资源管理器里多选图片 → 右键「格式转换」可整批转换：一个设置对话框依次处理全部图片，单张失败不中断，结束后汇总成功/失败数量
- 转换设置（旋转 / 缩放 / 格式 / 质量 / 输出位置 / 文件名前缀）会持久化，重启后沿用；主窗口与独立转换窗口共用同一份持久化设置，任一窗口的修改都会实时同步到另一窗口
- 可独立成窗口：`hive-viewer.exe --convert <图片路径...>`（可传多个路径；右键菜单多选时内部走 `--convert-list <临时列表文件>`），或使用右键菜单「格式转换」
- 若 `--convert` 没有有效图片路径，或 `--convert-list` 的清单文件丢失/不可读，不会静默打开主窗口，而是打开转换窗口并显示错误提示

**界面**

- 9 套主题（跟随系统 / 黑色 / 白色 / 暖阳 / 暮夜 / 森林 / 海洋 / 樱花 / 石墨），可自定义字体
- 四种语言：简体中文 / 繁體中文 / English / 日本語
- 快捷键全部可自定义，点击即录制，自动检测冲突
- 工具栏可锁定、窗口可置顶、支持全屏
- 内置版本号；可选「自动获取更新」：启动时后台静默检查 GitHub Release，有新版本弹一条轻提示，点一下即打开下载页
- Windows 11 右键菜单：打开图片 / 格式转换，可在设置里随时开关

## 安装

### 直接下载

到 Releases 下载 `HiveViewer_<版本>_x64-setup.exe`（NSIS 安装包），安装过程中会展示 MIT 协议。

系统要求：Windows 10 / 11 x64。需要 WebView2 运行时（Windows 11 自带；安装包会自动处理缺失的情况）。

### 从源码构建

| 前置依赖                    | 要求                                                |
| --------------------------- | --------------------------------------------------- |
| Node.js                     | 20.19+ 或 22.12+（Vite 8 要求）                     |
| pnpm                        | 10（仓库通过 `packageManager` 锁定版本）            |
| Rust                        | stable，≥ 1.77.2（Tauri 2.11 的 MSRV），MSVC 工具链 |
| Visual Studio 2022 生成工具 | 「使用 C++ 的桌面开发」工作负载 + Windows SDK       |

```bash
pnpm install
pnpm tauri dev      # 开发模式启动
pnpm tauri build    # 构建 NSIS 安装包
```

安装包输出在 `src-tauri/target/release/bundle/nsis/`。

如果要一并构建 **Windows 11 右键菜单扩展**（编译 COM 处理程序 DLL、打包并签名稀疏包 msix），用发布脚本：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-release.ps1
```

脚本会创建（或复用）一个自签名证书并把信任写入本机 `LocalMachine\TrustedPeople`，因此需要管理员权限；最后它还会去掉安装包文件名中的空格。

## 使用

### 快捷键

默认快捷键如下，全部可在 **设置 → 快捷键** 中点击重新录制（Esc 取消，自动检测冲突）：

| 操作                           | 默认快捷键                      |
| ------------------------------ | ------------------------------- |
| 上一张 / 下一张                | `←` / `→`                       |
| 顺时针 / 逆时针旋转            | `Ctrl + R` / `Ctrl + L`         |
| 全屏 / 退出全屏                | `F11` 或 `Alt + Enter`          |
| 打开 / 关闭 EXIF 面板          | `Tab`                           |
| 停止幻灯片                     | `Ctrl + 0`                      |
| 幻灯片间隔 1~9 秒              | `Ctrl + 1` … `Ctrl + 9`         |
| 删除当前图片                   | `Delete`（固定）                |
| 关闭面板 / 停止放映 / 退出全屏 | `Esc`（固定，按优先级逐级退出） |

鼠标：滚轮翻页（300ms 冷却），`Ctrl + 滚轮`缩放。

### 设置

设置面板分五个页签：

- **一般** — 删除确认、到首/末张行为、Esc 退出程序、窗口置顶、多实例、自动获取更新（默认关闭）与版本号
- **主题** — 9 套主题与字体，改动即时生效
- **快捷键** — 录制 / 重置
- **上下文菜单** — 总开关加「打开图片」「格式转换」分开关（注册 / 注销右键菜单包需要管理员权限，重新开启后可能需重启资源管理器）
- **语言** — 简体中文 / 繁體中文 / English / 日本語

所有设置持久化在 `%APPDATA%\dev.hive.viewer\settings.json`；转换对话框的设置单独存在同目录的 `convert-settings.json`（独立转换窗口也要能保存，两个窗口共用同一份持久化文件，同时修改时以最后保存的为准）。

## 项目结构

```
hive-viewer/
├─ src/                      # Vue 3 前端
│  ├─ components/            # 查看器、工具栏、设置面板、转换对话框等
│  ├─ composables/           # 工具栏显隐、通用设置副作用
│  ├─ i18n/                  # 四语言词条（键完整性由 TS 编译期保证）
│  ├─ stores/viewer.ts       # Pinia 状态：文件列表、分组、缩放、幻灯片、设置
│  ├─ update.ts              # 版本比对与启动时的静默更新检查
│  └─ styles/                # 主题变量与全局样式
├─ src-tauri/                # Rust 后端与打包配置
│  ├─ src/lib.rs             # 全部 Tauri 命令（打开/扫描、EXIF、转换、删除、右键菜单注册）
│  ├─ vendor/dav1d-shim/     # 纯 Rust dav1d 垫片（re_rav1d），供 image 的 avif-native 使用
│  ├─ windows/               # NSIS 模板、AppxManifest、稀疏包注册脚本
│  └─ tauri.conf.json        # 窗口、资源、文件关联、NSIS 配置
├─ shell-ext/                # Windows 11 右键菜单 IExplorerCommand 实现（cdylib）
├─ scripts/build-release.ps1 # 一键发布构建
└─ assets/                   # 图标源文件
```

## 开发

| 命令                           | 说明                                        |
| ------------------------------ | ------------------------------------------- |
| `pnpm dev`                     | 只启动前端（Vite，`http://localhost:5174`） |
| `pnpm build`                   | `vue-tsc` 类型检查 + 构建前端到 `dist/`     |
| `pnpm tauri dev`               | 启动完整桌面应用（含 Rust 后端）            |
| `pnpm tauri build`             | 构建生产安装包                              |
| `cargo check` / `cargo clippy` | 在 `src-tauri/` 下检查 Rust 代码            |

代码风格由 oxlint + stylelint + oxfmt 保证，通过 husky + lint-staged 在提交时自动执行；也可以手动全量跑一遍：

```bash
pnpm exec oxlint --fix --fix-suggestions .
pnpm exec stylelint "src/**/*.{css,vue}" --config .stylelintrc.mjs --fix
pnpm exec oxfmt .
```

## 已知限制

- 仅支持 Windows；右键菜单扩展依赖 Windows 11 的稀疏包（sparse package）机制
- 右键菜单需要在安装版中注册（开发环境不可用），且注册需要管理员权限
- 仓库暂无端到端测试（仅有 20 个 Rust 单元测试）

## 许可证

[MIT](LICENSE) © 2026 aethel-tail

发行版包含第三方组件（re_rav1d、mp4parse、Tauri 等），其许可证与版权声明见 [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)，该文件随安装包一同分发。
