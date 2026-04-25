# Changelog

All notable changes to BentoDesk are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.2.2] — 2026-04-25 · **Zone UX / Stack / Layout Repair Closeout**

继 0422 真实录屏暴露的 zone 堆叠 / 批量操作 / 边缘唤醒 / 图标空白 / 名称可读 / 布局恢复一组耦合性问题，本版完成一次"完整收尾"修复，重点不在新增功能，而在把现有交互闭环到生产级稳定。

### Fixed — 修复

#### Stack v2 渲染模型
- 将 stack 从"多个完整 BentoZone 嵌套在 transform wrapper 内"改造为"单一 StackCapsule + StackTray + 单成员 FocusedZonePreview"，**消除多 panel 同时抢交互**导致的视觉/命中混乱。
- `stack_zones` 现主动拒绝 < 2 输入（之前只拒绝 empty），消除孤儿 stack badge 残留的隐式契约——不再依赖 `normalize_zone_layout` 启动时清理。

#### 升级后图标空白与桌面布局错位
- `bentodesk://icon/{hash}` cache miss 现返回 **HTTP 404** 而非"看似成功的透明 1×1 PNG"——前端可识别失败并触发 fallback / 重新提取，不再误判为成功渲染。
- 新增 `repair_item_icon_hashes` Tauri 命令：遍历 layout 内 items，对空 hash / cache miss / path 已变化三种情况重新计算并回写。App 启动阶段 `repairItemIconHashes()` + `normalizeZoneLayout()` 通过 `Promise.allSettled` 并行调用，失败隔离不阻塞 UI。
- 新增 **`RestoreIdentity` 5 档优先级链**（`Original` → `Hidden` → `DisplayName` → `AmbiguousDisplayName` → `Unrecognised`），覆盖 spec G "稳定身份恢复"契约。`restore_zone_items` 已切换为该链，**同名多文件场景不再误恢复**——而是计入 skipped 报告并跳过。

#### 批量操作闭环
- `BulkZoneUpdate` IPC 结构补 `icon: Option<String>` 字段（前端 `BulkZoneUpdate` 接口、后端 `apply_bulk_updates` 分支、`BulkManagerPanel` UI 组件 + IconPicker 触发卡片，全链路对齐）。BulkManager v2 五字段 `icon / alias / display_mode / size / lock` **全部齐全**。
- 多 zone 实时整体拖动：`beginGroupZoneDrag` / `updateGroupZoneDrag` / `endGroupZoneDrag` 全链路接入 `BentoZone.tsx` 渲染管线，拖动期间所有选中 zone 实时跟随预览（不再 mouseup 后跳位）。

#### 边缘唤醒分流
- `computeInflateForPosition` 对 stack capsule 与普通 capsule 分别返回 inflate（zone box 160×48 / stack 184×56；zone edgeThreshold 120 / stack 132），靠近屏幕边缘时**只向屏幕外缘扩张，不向内侵蚀**。
- `hitTest.test.ts` 测试集扩展到 32 case（含 4 边 × 2 类型 + magnitude 区分）。

#### 动画与兼容性
- `prefers-reduced-motion: reduce` 覆盖率 **20/20 = 100%**（核心 6 + 外围 14）：BulkManagerPanel / ItemCard / SettingsPanel / PromptModal / MiniBarView / IconPicker / SmartGroupSuggestor / About / ZoneEditor / ZenCapsule / ContextMenu / SearchBar / SnapshotPicker / PanelHeader 全部追加。
- 低性能降级：`runtimeHealth.ts` 的 `data-runtime-effects="reduced|minimal"` 信号现接入 CSS 消费层（`spring-expand` / `content-reveal` / `item-lift` / `scale-in` / `pulse` / `item-enter`），从"信号断头路"补成端到端降级路径。
- 注：`spring-expand` 保留 v1.2.0 的 `width/height` 实现（`contain: layout paint` + `@property --rad` 合成层调优），不强行切到 transform-only。

### Added — 新增

- **图标库扩充**：`lucide-static` 升级 `0.471.0 → 1.11.0`，`icon-index.json` 重生成 **1947 条**（≥1600 spec 要求）。
- **`RestoreZoneItemsReport` / `SkippedRestoreItem` / `SkippedRestoreReason`** 三个新公共类型，让恢复路径的跳过状态对调用方可见，便于未来 UI 层提示。
- **`resolve_restore_identity(item, desktop_dir, hidden_dir)`** + 6 个 case 的纯函数测试，作为 spec G 契约着陆点。
- **`apply_stack_zones` / `apply_unstack_zones` / `apply_reorder_stack`** 抽出为纯 helper，`stack_zones` Tauri 命令成为薄壳——lock-mutate-persist 链路可独立单测。

### Tests — 测试覆盖

- 后端 `cargo test --lib`：**315 passed / 0 failed**（v1.2.1 时 ~270 → 现 315，**新增 ~45 个**）：
  - `commands/bulk.rs`：13（含 icon 字段全链路 + 5 种 layout 算法）
  - `commands/icon.rs`：5（empty / cache miss / path 变 / no-op / 边界）— 此前 0
  - `commands/item.rs`：6（`RestoreIdentity` 5 档 + 边界）— 此前 0
  - `commands/layout.rs`：6（clamp / reindex / drop singleton / collision / overflow / report）
  - `commands/zone.rs`：7（含 stack create / unstack / reorder / 单成员拒绝 / 跨 stack 转移）
  - `hidden_items.rs`：1 集成测试（ambiguous distractor 不误恢复）
- 前端 `pnpm test --run`：**192 passed / 0 failed**（v1.2.1 时 ~110 → 现 192，**新增 ~82 个**）：
  - `services/__tests__/hitTest.test.ts`：32（4 边 × 2 类型 + magnitude）
  - `services/__tests__/stack.test.ts`：扩充 9 case（60% 阈值 + tray open + stackMap）
  - `services/__tests__/runtimeEffects.test.ts`：9（dataset 信号 + 选择器命中）
  - `services/__tests__/ipc.test.ts`：扩充（`bulkUpdateZones` icon 字段透传 + 类型探针）
  - `__tests__/spec-matrix.test.ts`：**41**（spec 9 项 × 视频 6 时间点融合矩阵）
- `cargo clippy --lib --tests --all-features -- -D warnings`：**0 警告**
- `npx tsc --noEmit`：**0 错误**
- `pnpm build`：成功（CSS 87.92 kB / JS 459.40 kB / icon-index 1947 条）

### Internal — 工程结构

- 新增 `src/components/BentoZone/StackCapsule.tsx` / `StackTray.tsx` / `FocusedZonePreview.tsx`
- 新增 `src/services/groupDrag.ts` + `src/stores/stacks.ts`
- 新增后端命令 `repair_item_icon_hashes` / `normalize_zone_layout`
- 新增公共类型 `RestoreIdentity` / `RestoreZoneItemsReport` / `LayoutNormalizeReport.skipped`

### 升级注意

- v1.2.1 → v1.2.2 走自动 updater（minisign 签名验证）。
- 旧布局加载 → 启动 `repairItemIconHashes` + `normalizeZoneLayout` 自动修复透明图标 / 越界位置 / 同名 ambiguous item。
- `BulkZoneUpdate.icon` 是新增字段，老前端发送旧 payload 仍兼容（serde `default`）。

---

## [1.2.1] - 2026-04-22

补丁版，面向已经安装 `v1.2.0` 但遇到启动闪退或开机自启失效的用户。

### Fixed

- 修复 Windows 安装包在启动阶段因异步运行时上下文不正确而直接闪退的问题。
- 修复开机自启在以下场景下失效的问题：`guardian.exe` 非有效可执行文件、Task Scheduler `/tr` 命令过长、`/delay` 参数格式错误、当前用户上下文下 `schtasks` 被拒绝访问。
- 当 Task Scheduler 创建失败时，自动回退写入 `HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run`，避免“设置已保存但实际上未生效”。
- 设置页保存开机自启失败时，后端会回滚本次设置改动，前端会显示错误提示，而不再表现为假成功。

## [1.2.0] — 2026-04-18 · **Silent Power + Bulk Elegance + Memory Discipline**

v1.2.0 把 v1.1 打磨到生产级日用，并用 5 个差异化杀手功能拉开与 Fences 的代差。

> ⚠️ **首次升级需手动安装一次**：v1.1.0 未内嵌 updater 插件，因此 v1.1.0 → v1.2.0 无法自动更新。请从 Releases 页手动下载 `BentoDesk_1.2.0_x64-setup.exe` 安装覆盖。安装 v1.2.0 后，**未来所有版本 (v1.2.1+ / v1.3.0 / …) 都将自动丝滑更新**。

### 🛡️ 升级安全保证（v1.1.0 → v1.2.0）

| 保证项 | 实现方式 |
|---|---|
| ✅ 所有 zones / items / 布局原封保留 | `Desktop\.bentodesk\manifest.json` schema 向后兼容 (`#[serde(default)]` + `_legacy` 残留字段托管) |
| ✅ settings.json 自动升级，无需用户介入 | v1.2.0 启动时 `load_or_default` 先 pre-migration 备份 → 跑 `migrate_v1_to_v2` → 未知字段存入 `_legacy` 命名空间（downgrade 可读） |
| ✅ 破坏性迁移自动回滚 | `.bak` 兄弟文件 + rotated `settings.backup.<ts>.json` (保留最新 3 份)，迁移失败自动恢复 |
| ✅ Time-machine 历史（timeline）保留 | ring buffer 二进制格式未变 |
| ✅ icon cache 保留 | hot/warm 分层 cache 对旧 hot-only 数据向后兼容 |
| ✅ 升级不触发"卸载 BentoDesk"的弹窗与数据清理 | **NSIS uninstall hook 用 `IfSilent` 区分升级 vs 真·卸载**——Tauri v2 升级流程的静默卸载会跳过所有数据清理逻辑；真·用户卸载（点 Start Menu 的 `卸载 BentoDesk`）才执行完整清理 |

**如何手动升级**（一次性操作）：
1. 关闭正在运行的 BentoDesk（右键托盘图标 → 退出）
2. 从 [Releases](https://github.com/ZRainbow1275/bentodesk/releases/latest) 下载 `BentoDesk_1.2.0_x64-setup.exe`
3. 双击安装——安装器会静默卸载 v1.1.0 再装 v1.2.0，**你的 settings / zones / 图标位置全部保留**
4. 启动 v1.2.0，首次运行会在 `%APPDATA%\BentoDesk\` 创建 pre-migration 备份

### 🚑 意外数据丢失的手动恢复

即使上述保证全部到位，如遇极端情况（磁盘故障 / 安装中断 / 其他未知 bug）：

**找回 settings**：
1. 打开 `%APPDATA%\BentoDesk\` 目录
2. 找到最新的 `settings.backup.<timestamp>.json`（保留最新 3 份）
3. 复制一份为 `settings.json`（覆盖现有损坏文件）
4. 重启 BentoDesk

**找回桌面文件**：
- v1.1 整理的文件仍在 `桌面\.bentodesk\` 隐藏文件夹内，**从未被卸载/升级流程删除**
- 在文件管理器"显示隐藏文件" → 可查看/恢复

**完整回滚到 v1.1.0**（不建议，仅极端情况）：
1. 从 [Releases](https://github.com/ZRainbow1275/bentodesk/releases/tag/v1.1.0) 下载 v1.1.0 installer
2. 手动卸载 v1.2.0（Start Menu → 卸载 BentoDesk，确认清理）
3. 安装 v1.1.0
4. settings.backup 中的 v1.1 字段已通过 `_legacy` 命名空间保留，v1.1.0 启动时会读回

### Added — 新增功能

#### A · 丝滑更新 + 设置安全存储
- **Tauri Updater 集成** — 基于 `tauri-plugin-updater@2` + GitHub Releases endpoint + minisign 签名验证，支持静默下载、重启安装、跳过版本、手动/每日/每周检查频率
- **设置备份与恢复** — `%APPDATA%\BentoDesk\settings.backup.<ts>.json` 滚动保留最新 3 份；启动前自动创建 pre-migration 备份；破坏性迁移可自动回滚
- **Schema additive migration** — v1.1 残留字段移入 `_legacy` 命名空间而非删除，降级可读
- **可选加密存储** — DPAPI (Windows 原生，默认) / AES-256-GCM + Argon2id (跨机便携，用户 passphrase) / None 三档可选；模式切换前双向 roundtrip probe 防止锁死
- **`zeroize` 敏感数据擦除** — 加密密钥与 passphrase 离开作用域立即清零

#### B · 性能与内存纪律
- **胶囊伸缩 GPU 加速动画** — `transform: scale()` + `@property --rad` + `will-change`，P95 FPS ≥ 55
- **分层图标缓存** — hot (内存 LRU) / warm (磁盘 `%APPDATA%\icon_cache`) / cold (重新提取)
- **Zone 虚拟滚动** — item > 20 时启用 `IntersectionObserver` 只渲染可视区域
- **空闲 GC** — app idle 60s 主动触发 icon cache sweep
- **WebView2 内存监控** — 每 30s 采样，超 200MB 托盘建议重启
- **DPI-adaptive 图标尺寸** — 按屏幕 DPI + zone size 按需提取 16/24/32/48/64 档
- **`prefers-reduced-motion` / `prefers-reduced-transparency`** 支持

#### C · 快捷键 + 批量操作
- **6 个全局快捷键** — `Ctrl+Shift+N/D/R/L/H` + `Ctrl+[/]` focus 切换
- **12 个场景快捷键** — `Del/F2/Ctrl+D/Alt+方向键/Enter/Shift+Del`
- **冲突检测 + 可视化重绑定面板** (`KeybindingsPanel.tsx`)
- **多选机制** — `Shift+Click` 连选 / `Ctrl+Click` 加选 / 空桌面拖选框
- **批量拖拽** — 保持相对位置平移；`Alt+drag` 复制而非移动
- **4 种自动排布算法** — Grid / Row-Column / Spiral / Organic (physics-based 吸附)
- **批量 undo checkpoint** — `Ctrl+Z` 一键还原批量操作
- **`BulkZoneManager.tsx`** — 表格视图 + 批量颜色/大小/显隐/锁定/筛选清理 (`Ctrl+Shift+M`)

#### D · UX 打磨
- **Hover 唤醒方向修复** — 胶囊位于屏幕边缘时，展开方向动态贴边（不再向任务栏溢出）
- **方向性 hit zone 扩展** — 屏幕边缘胶囊向外扩 12px，包含任务栏边缘
- **速度快通道** — 鼠标速度 > 800 px/s 跳过意图判定立即 expand
- **DebugOverlay** — 设置开启后实时显示 hit rect / inflate box / anchor glyph
- **Zone Stack Mode** — 两 zone 重叠 > 60% 自动堆叠；扇形展开 / 分离动画 / 持久化 `stack_id`
- **智能名称缩写** — ASCII 多词取首字母 / CJK 取首 1-2 字；Canvas `measureText` 精确测宽；Portal tooltip 显示完整名
- **Zone 别名** — 右键 → "Set Alias"，仅显示用，不改文件夹名

#### E · 图标库 + 差异化
- **Lucide Icons 集成** — 1600+ open-source 图标（MIT），分类/搜索/最近使用/自定义 SVG 上传
- **Context Capsule** — `Ctrl+Alt+C` 保存工作流快照（窗口位置 + zone 展开态 + 选中项），`Ctrl+Alt+V` 恢复。场景：会议模式 / 编码模式 / 设计模式一键切换
- **AI Zone 推荐** — 基于桌面文件命名/扩展名模式，本地规则引擎（可选 Ollama LLM）生成分组建议
- **Pin-to-Top Mini Bar** — 把 zone 钉为浮动迷你工具条，半透明 + 屏幕边吸附
- **规则化自动归档** — Outlook 风格条件链 + 动作链（"创建时间>7天+未访问 → 移入 Archive"，".tmp/.log → 移入 Trash 每周清理"）；所有自动操作可 Time-machine 撤回
- **Live Folder Sync** — Zone 可镜像任意磁盘文件夹（不限桌面），双向同步重命名

### Fixed — 修复（v1.2.0 Polish Bugfix）

#### 核心显示 Bug
- **Zone 边缘展开方向错位** — 胶囊位于屏幕右/下边缘时，展开 panel 不再错误地"跑到屏幕上方/左侧"。`captureAnchorSnapshot` 翻转判定从激进阈值（`panelH + MARGIN = 428px`）改为"自然侧放不下 AND 翻转侧放得下"才 flip，彻底消除 y_percent > 60% 即翻转的 bug
- **胶囊拖拽无法贴屏幕底边** — drag clamp 写死 `95%` 被替换为基于胶囊实际尺寸动态计算 `100 - capsuleW/H%`，现在可拖到真正的屏幕边缘
- **多显示器混合 DPI 定位错位** — `captureAnchorSnapshot` 优先使用 `MonitorInfo.dpi_scale`，回退 `window.devicePixelRatio`；副屏非主屏 DPI 场景 work-area 计算不再偏数百 px

#### Stack Mode Bug
- **StackWrapper CSS 坐标覆盖** — `.stack-wrapper` 由 `inset:0` 改为通过 `--stack-x/--stack-y/--stack-w/--stack-h` CSS vars 继承栈底 zone 的坐标，堆叠不再错误地渲染在屏幕 (0,0)
- **双击栈顶应只弹顶** — 之前 dblclick 解散整个栈，现改为弹出 `stack_order` 最大的成员，其余保持堆叠（2 成员时 fallback 整组解散）
- **Shift+drag 应协同移动** — 之前 Shift+mousedown 立即 unstack，现改为锁定 dragLock → 批量 `updateZone` 所有成员 position 加 delta，不解散 stack_id

#### 数据安全 Bug
- **Settings migration 遗漏两字段** — `KNOWN_1_2_FIELDS` 追加 `debug_overlay` 与 `zone_display_mode`，防止降级-升级循环把它们塞进 `_legacy`
- **布局算法不更新时间戳** — `apply_layout_algorithm` 现对每个受影响 zone bump `updated_at`，layout time-machine 能正确捕获批量布局变更
- **规则批处理无法撤销** — `rules/executor.rs` 的 `timeline_hook::record_change` 从 mutation 前移到 mutation 后，确保 snapshot delta 非空，Ctrl+Z 能回滚整个规则批

#### 代码质量
- `BentoZone.tsx` `createMemo` 副作用反模式 → `createEffect`
- `computeInflate` 抽到 `hitTest.ts` 单一来源，`BentoZone` 与 `DebugOverlay` 复用
- `ContextMenu` 的 `window.prompt()` 在 Tauri WebView2 passthrough 下可能阻塞 → 替换为 `PromptModal` (Portal + focus trap + Enter/Escape)

### Quality Gates

| Gate | Result |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | ✅ 零警告 |
| `cargo test --lib` | ✅ **263/263** |
| `pnpm tsc --noEmit` | ✅ 0 errors |
| `pnpm test` | ✅ **115/115** (9 test files) |
| `pnpm vite build` | ✅ 118 modules / 282.51 kB |

### Known Limitations

- **Stack 内 zone 展开**：`.stack-wrapper__layer` 的 `transform` 会让 `getBoundingClientRect()` 返回变换后 rect，flip 判定可能微抖。建议 v1.3 让 stack 内 zone 展开前 unstack
- **Anchor 切换无过渡**：从 `top: X%` 直接切 `bottom: Ypx` 是瞬时布局切换（动画禁止 left/top 是为了避免与 drag 冲突）
- **`window.alert` 残留少量路径**：Ctrl+Z 错误提示仍用浏览器原生 alert，v1.3 统一为 Toast

---

## [1.1.0] — 2026 · Safe Automation + Clean Desktop

详见 git log `6e28bcc`。
