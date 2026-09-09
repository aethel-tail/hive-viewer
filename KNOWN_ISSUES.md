# Hive Viewer 已知问题 / Known Issues

本文件记录 1.2.0 发布前代码评审（3 个 fresh-context reviewer）发现、但**未在 1.2.0 中修复**的 P2 级问题，以及几项刻意取舍。它们都不是阻塞项，但复现路径和修复方向已写明，便于后续排期。

- 修复批次：`c2d6057`（跨窗口同步 / 错误态 / 失败明细 / AVIF 限定）
- 版本批次：`eb793ea chore(release): 1.2.0`
- 已修复并验证的项（P1/P2）见上述 commit 与 1.2.0 release notes，不在此重复。
- 本次 council 决议：下面每个 P2 问题已附「解决方案（council 决议）」；AVIF 输入从刻意取舍改为支持（方案见该节）；前端自动化测试基建按 owner 要求不在本次范围。
- 1.2.1 已按下方「解决方案（council 决议）」实现（5 个 P2 + AVIF 输入支持）；前端自动化测试基建仍不在范围内。

## 未修复的 P2

### 1. 批量转换进行中收到新批次 → 旧批完成时写旧汇总

- **位置**：`src/components/ConvertDialog.vue`（旧结果清理 watcher 的 `busy` 守卫；`doConvert` 结束写 `batchSummary` / `errorMsg`）
- **现象**：大批次转换中，从资源管理器再发一批。队列与预览已切到新批次，但旧批完成时会把**旧批**的汇总/失败明细写进 UI，直到再次点“开始转换”。
- **影响**：UI 误导，无数据损坏。
- **复现**：转换窗口跑 3 张大图 → 期间资源管理器再选 3 张右键「格式转换」→ 旧批结束后面板显示旧汇总。
- **建议**：`busy` 期间 watcher 只记录“队列已变”，等 `busy` 翻 false 后再清一次；或 `doConvert` 结束时比对快照队列与当前队列，不一致就不写结果。

- **解决方案（council 决议，Pass 1+2 一致）**：
  - 在 `ConvertDialog.vue` 引入单调递增 `runSeq`（epoch）。`doConvert` 开始时 `const runId = ++runSeq`，并快照 `const list = targets.value.slice()` 与 `runTotal = list.length`；所有 await 之后的 UI 写入（逐项进度、`savedPath`、`batchSummary`、`errorMsg`、`store.showToast`、单张成功后的 `emit("close")` / `store.openImageByPath`）先判 `runId === runSeq`，不等则跳过。
  - 清理 watcher 在 `busy` 时不再静默 `return`：置 `pendingReset = true`（并用 store 级 `runId` 标记被取代的运行）；`doConvert` 的 `finally` 里若 `pendingReset` 则清空结果面板并复位标记。进度与 `isBatch` 使用快照 `runTotal` / 快照队列，避免“旧批进度 + 新批计数”。
  - 新批次不自动开始（`busy` 禁用 Start）；旧批继续跑完。旧批结果不再写进 `errorMsg` / `batchSummary`，而是走**两阶段常驻状态条**：busy 时显示“上一批仍在转换 done/total”，旧批结束后替换为最终汇总 + 最多 5 条失败明细（复用 `MAX_FAILURES`），直到用户点「开始转换」或手动关闭；失败信息不会像 toast 一样自动消失。
  - **验收**：转换中收到新批次 → 结果面板不出现旧批汇总/错误；旧批结束不干扰新批；点开始后新批正常转换；单张路径行为不变；旧批失败明细仍可见。

### 2. 同一 IPC 往返内的并发保存会覆盖（整对象 set）

- **位置**：`src/stores/viewer.ts` `saveConvertSettings()` → `tauri-plugin-store` 的 `set` + `save`
- **现象**：两个窗口几乎同时修改**不同字段**时，两次整对象写入收敛到最后一次，前一次字段被静默回退。跨窗口实时同步已把窗口缩小到“同一 IPC 往返”，但没有消除。
- **影响**：极端情况下转换设置回退一格。
- **建议**：改为逐字段 `set`；或保存前读取磁盘现值做 merge；或维护 per-field dirty 集合只写变更键。

- **解决方案（council 决议，选 B：Rust 原子 patch 命令）**：
  - `src-tauri/src/lib.rs` 新增命令 `update_convert_settings(app, patch)`：进程级 `static Mutex<()>` 串行化；`StoreExt::store("convert-settings.json")` 拿到与前端 `LazyStore` 同一个 `Arc<Store>`；读出当前 `convertSettings`，只 merge 白名单字段，`set` + `save`，返回合并结果。把 merge 逻辑抽成纯函数 `merge_convert_patch(current, patch) -> Result<JsonValue, String>` 并写 Rust 单测（不同字段合并不丢、未知字段拒绝、`width: null` 清空、quality clamp）。
  - `viewer.ts`：`convertSettings` 仍作为 reactive UI 模型；维护 `lastSaved` 快照，deep watch 时 diff 出变化字段，只 `invoke("update_convert_settings", { patch })`；成功后更新快照；远端 `onKeyChange` 回灌时也更新快照，保留 `skipNextConvertSave` 防回环。删除 `saveConvertSettings()` 里的 `convertStore.set("convertSettings", {...})` 整对象写入——该命令必须是 `convertSettings` 的唯一写入口（`viewer.ts:950` 是当前唯一调用点，替换后用 grep 复查）；`convertStore` 只保留 `get` / `onKeyChange`。独立转换窗口共用同一 store 模块，自动覆盖。
  - **备选 A（平铺 per-field key + `onChange` + 迁移）不选**：文件形状变化，11 个字段的监听/迁移/sanitize 改造都落在无前端测试的代码里；**C（前端 get-merge-set）非原子**，拒绝。
  - **验收**：两窗口同时改不同字段，二者都落盘且重载后都在；无同步死循环；旧 `convert-settings.json` 可读；单测覆盖 merge；同字段并发仍为 last-writer-wins（可接受）。

### 3. 转换进行中收到空 payload（第二实例 `--convert-list` 失效）会卸载对话框

- **位置**：`src/ConvertWindow.vue` `acceptBatch([])` → `clearConvertBatch()`；`src/components/ConvertDialog.vue` `doConvert`
- **现象**：旧批仍在后台转换，UI 已切到 `convert.listError` 错误面板；旧批完成时结果写到已卸载的组件 ref。
- **影响**：视觉不一致（错误面板 + 文件其实写成功了），无数据损坏。
- **复现**：转换进行中，另一个实例传入已失效的 `--convert-list`（清单文件被清理）触发空 payload。
- **建议**：`busy` 时不要清空队列，只在错误面板提示；或先取消/等待旧批再切换。

- **解决方案（council 决议，Pass 1+2 一致）**：
  - `stores/viewer.ts` 增加 `convertBusy`（`ref(false)`，不持久化）与 store 级 `runId`。`doConvert` 在第一个 await 前同步置 `convertBusy = true`，`finally` 置 false；**不要在 `onUnmounted` 里复位**——主窗口对话框卸载后旧批仍在跑，复位会让重新打开的对话框误判空闲并启动第二个并发批次。store 级 `runId` 在卸载/换批时递增，用于让旧批的 UI 写入失效。
  - `ConvertWindow.acceptBatch([])`：若 `store.convertBusy`，不要 `clearConvertBatch()`，保持队列与 `ConvertDialog` 挂载；显示非破坏性的 `listWarning` 状态条（两阶段：busy 时提示“上一批仍在转换”，旧批结束后显示其结果/失败明细）；只有空闲时才走原来的 `clearConvertBatch()` + 全屏错误面板。任何非空批次到达时清除 warning，窗口标题保持旧队列的「N 张」。
  - 关闭策略（owner 决策：要弹确认）：不硬阻断关闭；busy 时关闭对话框弹一次确认「转换进行中，关闭后仍会继续」，确认后才关闭；旧批不提供取消，跑完为止。
  - **验收**：转换中收到空 payload → 对话框不卸载、旧批进度/结果可见 + 状态条；空闲时收到空 payload → 仍显示全屏错误窗口；主窗口中途关闭再打开不会启动第二个并发批次；非空批次清 warning。

### 4. README 未记录“空路径错误窗口”

- **位置**：`README.md` 的 `--convert` / `--convert-list` 说明；`AGENTS.md` 已写。
- **建议**：README 补一句：无有效图片路径时不再静默开主窗口，而是打开转换窗口并显示错误。

- **解决方案（council 决议）**：在 `README.md` 的格式转换/CLI 部分（`--convert` bullet 之后，约 line 40）补一条：「若 `--convert` 没有有效图片路径，或 `--convert-list` 的清单文件丢失/不可读，不会静默打开主窗口，而是打开转换窗口并显示错误提示。」`已知限制` 不需要改（这是行为说明，不是限制）；AVIF 输入支持落地后同时改写 line 36-37 的 AVIF 限制说明。**验收**：README 与 `AGENTS.md`、`is_convert_invocation` 行为一致。

### 5. `convert.listError` 补救措辞偏 Explorer

- **位置**：`src/i18n/zh-CN.ts` / `zh-TW.ts` / `en.ts` / `ja.ts` 的 `convert.listError`
- **现象**：文案只说“请重新在资源管理器中执行转换”，但命令行 `hive-viewer.exe --convert <坏路径>` 也会命中同一错误态。
- **建议**：改为“请重新执行转换（资源管理器右键或命令行）”。

- **解决方案（council 决议；owner 决策：采用明确版）**：四个语言文件同步更新（TS 编译期强制 key 一致），覆盖“资源管理器右键”和“命令行”两条触发路径。采用更明确的措辞：
  - zh-CN：`无法读取待转换的图片（临时清单可能已失效，或没有可用的图片路径）。请重新执行转换：在资源管理器中右键图片选择「格式转换」，或使用命令行 hive-viewer.exe --convert <图片路径>。`
  - zh-TW：`無法讀取待轉換的圖片（臨時清單可能已失效，或沒有可用的圖片路徑）。請重新執行轉換：在檔案總管中右鍵圖片選擇「格式轉換」，或使用命令列 hive-viewer.exe --convert <圖片路徑>。`
  - en：`Could not read the images to convert (the temporary list may have expired, or there were no usable image paths). Run the conversion again: right-click the images in File Explorer and choose "Convert format", or use the command line hive-viewer.exe --convert <image path>.`
  - ja：`変換する画像を読み取れませんでした（一時リストが無効になったか、有効な画像パスがありませんでした）。もう一度変換を実行してください：エクスプローラーで画像を右クリックして「形式変換」を選ぶか、コマンドライン hive-viewer.exe --convert <画像パス> を使用します。`
  - **验收**：四个 locale 均覆盖两种路径；`pnpm build` 通过。

## 刻意取舍（Deliberate decisions）

### AVIF 仅支持浏览与输出，不支持作为转换输入（本次 council 决议：改为支持输入，方案见下）

`image` 0.25 的 `avif` feature 只带编码器；解码需要 `avif-native`，会引入原生 `dav1d`（Windows 上还需 meson/ninja 工具链）。为保持依赖最小，当前不启用。`decode_image` 会对 AVIF 内容返回明确错误「暂不支持 AVIF 输入解码，请先转换为其它格式」，不会给出含糊的解码失败。将来若要支持，需评估原生依赖、CI 构建与安装包体积。

- **解决方案（council 决议，取代上述刻意取舍；主方案 C + 兜底 A）**：
  - **主方案 C（纯 Rust，无原生工具链）**：新增 `src-tauri/vendor/dav1d-shim/`，包名 `dav1d`、版本 `0.11.0`，依赖 `re_rav1d = { version = "=0.1.3", default-features = false, features = ["bitdepth_8", "bitdepth_16"] }`，`src/lib.rs` 仅 `pub use re_rav1d::dav1d::*;`；`src-tauri/Cargo.toml` 给 `image` 加 `avif-native`（保留 `avif`），加 `[patch.crates-io] dav1d = { path = "vendor/dav1d-shim" }`、`[profile.release.package.re_rav1d] opt-level = 3`，提交 `Cargo.lock` 锁版本；删除 `decode_image` 的 `ImageFormat::Avif` 拒绝分支（保留内容嗅探），AVIF 失败返回明确错误。已在本机验证：`cargo check` 通过；64×64 与 alpha 渐变 AVIF 编码→解码往返通过；随机/截断/损坏输入全部返回 Err、无 panic；release 下 1024×768 解码 ~10.2ms（no-asm）。
  - **容器方向（必须做；owner 决策：尽可能不要 unsafe）**：image 的 AVIF 解码器只取 `primary_item_coded_data()`，不应用 `irot`/`imir`。加直接依赖 `mp4parse = "0.17"`，用 `read_avif` 读 `image_rotation()`（安全枚举）映射为 EXIF orientation 码（irot D0/D90/D180/D270 → 1/3/6/8）；`imir` 优先用最小安全解析器直接读容器里的 `imir` box（4 字节：0=TopBottom，1=LeftRight，→ 4/2，与 irot 组合覆盖 8 种情况），避免依赖 `image_mirror_ptr()` 的裸指针；只有评估后确认无法安全读取时，才接受单处带 SAFETY 注释的 `unsafe { *ptr }`（`AvifContext` 存活期内），并保留“只支持 irot + 文档化 imir 限制”的退路。优先级：容器 irot/imir 存在时跳过 EXIF，避免双重旋转；`image_dims` 的宽高交换也要用同一套方向。
  - **不支持项**：grid AVIF（image 不拼 tile）、动画 AVIF（avis）返回明确错误；ICC/BT.2020-CL 等按 image 的能力处理并文档化。
  - **测试（Rust，不启动应用）**：用现有 `AvifEncoder` 生成小 AVIF → `decode_image`/`process_image` 断言尺寸/像素/alpha；10-bit、EXIF-Orientation-6、irot/imir、动画 fixture 可选（owner：无所谓；优先在测试里生成 8-bit，其余可提交极小二进制或标记手动/忽略）；把 malformed/截断/损坏 corpus 固化为确定性测试，并加 mutation（逐截断点、单字节翻转、box size/count 溢出、空 item）；发布前跑一次有界 `cargo-fuzz`（owner 已接受 2–4 CPU 小时），任何 panic 都阻断发布并触发兜底。
  - **性能门槛（绝对值，替代 WebP 比值）**：release 构建下 24MP AVIF 解码 ≤1s、AVIF→PNG ≤2s（开发机），并跑正确性 corpus（8/10/12-bit、alpha、irot/imir、EXIF、grid、malformed）。若 24MP 不达标：先启用 `re_rav1d` 的 `asm`（只需 NASM，无需 meson/ninja），仍不达标再切兜底 A。
  - **panic 策略（owner 决策）**：保持 `[profile.release] panic = "abort"`，以 fuzz/mutation corpus 作为发布门禁——任何 panic 阻断发布，先修复或触发兜底 A；不改为 `panic = "unwind"`（如后续真实使用中遇到 panic，再评估 unwind + `catch_unwind` 把崩溃降级为单文件错误，并注意 `PROXY` 锁的中毒问题）。
  - **兜底 A（C dav1d）**：`image` 的 `avif-native` + dav1d 1.3.0；CI/本地需 git + meson + ninja + NASM（或固定预编译/系统 dav1d + pkg-config），保留同样的 guard 移除与测试。B（预编译 libdav1d + DLL + BSD-2 声明）作为 A 的备选；D（WebView 桥）仅在原生路径全被封死时考虑，接受 8-bit/预览质量损失；E（WIC）排除。
  - **许可证（owner 询问的“第三方许可证声明”是什么意思）**：发行安装包时，二进制里包含第三方 crate；BSD-2 要求在分发时保留版权与许可证文本，MPL-2.0 要求提供被覆盖源码的获取方式并附带许可证。做法：新增 repo 根 `THIRD_PARTY_LICENSES.md`（列出依赖 + 许可证全文/链接，可用 `cargo-about` 生成），并在 NSIS 许可证页/安装包资源中引用或随 exe 分发；这不改变 Hive Viewer 自身的 MIT 许可，只是履行依赖的署名/源码可用义务。`re_rav1d` BSD-2、`mp4parse` MPL-2.0；`image` 0.25.10 无上游纯 Rust AVIF 解码；`zenavif`/`rav1d-safe` 为 AGPL-3.0-only 或商业授权，MIT 项目不可用；`zenavif-parse` 为 MPL-2.0；`gamut-avif` 只有容器/编码；`oxideav` 仍 0.0.x。
  - **owner 决策（已确认）**：主方案 C + 兜底 A；有界 fuzz 2–4 CPU 小时接受；panic 策略保持 `panic = "abort"` + fuzz/mutation 发布门禁（见上）；busy 关闭确认要弹（item 3）；imir 优先安全实现、实在不行才接受单处 unsafe；fixture 可选；第三方许可证落点 = `THIRD_PARTY_LICENSES.md` + 安装包引用；item 5 采用明确版。
  - **验收**：`cargo check`/`clippy`/`test` + `pnpm build` 通过；单张/批量转换、预览、旋转/缩放/各输出格式均可用；无 panic；README（含 line 36-37）与 `AGENTS.md` 更新；记录安装包体积/构建时间增量。

### 前端暂无自动化测试基建

项目没有 test runner（无 `test` script、无 vitest/jest）。1.2.0 的跨窗口同步、转换窗口错误态、旧结果清理与失败明细上限只有 Rust 侧 `is_convert_invocation` 与 AVIF 守卫测试覆盖；前端部分靠 `vue-tsc` 与构建兜底。若后续引入，建议 Vitest + 少量针对 `stores/viewer.ts` 与 `ConvertDialog.vue` 的用例。

> 本次 council 范围明确排除该项（前端自动化测试基建）；AVIF 方案里的 Rust 测试 / 有界 fuzz 属于验收项，不在排除范围内。
