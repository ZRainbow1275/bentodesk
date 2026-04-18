# Changelog

All notable changes to BentoDesk are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

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
