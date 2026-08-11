# BentoDesk 社区发布初稿

> 状态：初稿，2026-08-12。
> 用途：微信公众号、NodeSeek、Reddit、X 等平台的发布素材。
> 重要：**不要把本文原样发布到 Hacker News 或 Linux DO。** HN 的现行发布建议要求文字由作者亲手写；Linux DO 不允许直接发布 AI 生成或润色文字。文末只提供事实提纲与合规清单。

---

## 中文母稿

# BentoDesk 2.0：把桌面文件折进 Zone

## 1. 简介

BentoDesk 是我写的一款开源 Windows 桌面整理工具。文件、文件夹和快捷方式可以放进 **Zone**。Zone 平时收成一枚胶囊，悬浮或点击后在原地展开，用完再缩回去。

![BentoDesk 桌面演示](https://raw.githubusercontent.com/ZRainbow1275/bentodesk/main/docs/media/desktop-tour.webp)

完整演示：[33 秒 MP4](https://github.com/ZRainbow1275/bentodesk/blob/main/docs/media/desktop-tour.mp4)

BentoDesk 不建立专有文件库。文件仍由 Windows 管理，关掉软件后也还是普通的 Windows 文件。

## 2. 为什么做这个项目

我一直从桌面开始工作。临时文件放在眼前最好找，但放几天就会铺满屏幕；塞进层层文件夹后很整齐，过两天又容易忘掉。

我想要的东西很简单：文件还在手边，不用时别占着整张桌面。

1.x 使用 Tauri，先把这套交互跑通了。后来，动画反向、桌面点击穿透、Shell 拖放和多显示器适配都需要更直接地控制 Windows 窗口与渲染链路。我没有继续给 WebView 打补丁，而是用 Rust 和 Win32 重写了 2.0。

## 3. 功能介绍

- **Zone：** 胶囊和展开网格共用同一块原生界面。名称、别名、图标、颜色、列数、宽度和五种边角都能单独调整；展开方式可选悬浮、单击或常驻。
- **文件拖放：** 使用 Windows Shell/OLE 处理文件、文件夹和快捷方式。把项目拖出 Zone 后，只有 Shell 确认操作完成，BentoDesk 才会更新记录。
- **搜索与 Stack：** 可以搜索当前 Zone，也可以搜索全部 Zone。多个 Zone 能组合成 Stack，各自的布局和样式不会被抹掉。
- **批量整理：** 可以同时显示、隐藏、移动或删除多个 Zone，并按网格、横向、纵向、环绕或自然方式重新排列。
- **自动整理：** 智能分组只生成建议，由用户决定是否应用；Live Folder 和规则处理重复整理；本地主题插件支持安装、启停和删除。
- **恢复与设置：** 支持布局快照、时间线、设置备份和加密。批量删除、快照删除等操作需要确认。
- **外观与语言：** 支持明暗主题、强调色、动画和性能选项；界面可随时切换 English 与简体中文。

![Zone 展开与搜索](https://raw.githubusercontent.com/ZRainbow1275/bentodesk/main/docs/media/zone-motion.webp)

完整演示：[Zone 动画 MP4](https://github.com/ZRainbow1275/bentodesk/blob/main/docs/media/zone-motion.mp4)

## 4. 安装方式

支持 Windows 10 1809+ 和 Windows 11，仅提供 x86-64 版本。

### 安装版

从 [GitHub Releases](https://github.com/ZRainbow1275/bentodesk/releases/latest) 下载 `BentoDesk-2.0.8-windows-x64-setup.exe`。安装程序包含中英双语的用户协议与隐私政策，按当前用户安装，不需要管理员权限。

卸载时默认保留设置；只有主动勾选并再次确认，才会删除 BentoDesk 自身的本地状态。

### 便携版

不想安装可以下载 `BentoDesk-2.0.8-windows-x64-portable.zip`，解压到可写目录后直接运行 `BentoDesk.exe`。

安装版和便携版都不需要 Node.js、Tauri、WebView2 或单独的浏览器运行时。

两个下载包都可以用 Release 中的 `SHA256SUMS.txt` 校验。

## 5. 特色（对比其他产品或项目）

Fences 和 Portals 已经把桌面整理做得很成熟。BentoDesk 没打算照着它们补一张更长的功能表，而是选了一个更窄的方向：不用时尽量缩小，需要时就在原处打开。

| 方案 | 更适合的情况 |
| --- | --- |
| Windows 文件夹和桌面图标 | 不想安装额外软件，只需要系统原有的文件夹与快捷方式。 |
| [Stardock Fences 6](https://www.stardock.com/products/fences/) | 看重成熟的自动分类规则、Peek、Folder Portals、标签页或企业部署。 |
| [Portals](https://portals-app.com/) | 需要常驻文件夹面板、多标签、细致的面板样式和随显示器切换的布局。 |
| **BentoDesk** | 希望整理器平时尽量缩小；需要 Zone 原地展开、Stack、直接桌面文件操作、布局快照和时间线恢复；偏好开源、离线和原生 Windows 实现。 |

### 技术栈

| 项目 | 技术 |
| --- | --- |
| 语言 | Rust 2024 |
| Windows 与输入 | Win32 / USER32 / DWM |
| 图形与文字 | Direct2D / DirectWrite / DirectComposition |
| GPU 与交换链 | D3D11 / DXGI |
| 图标、图片与文件 | WIC / Windows Shell / OLE / `ReadDirectoryChangesW` |
| 本地数据 | DPAPI / 原子写入 / 加密设置 vault |
| 构建 | MSVC x64 / static CRT / `opt-level="z"` / Fat LTO |

2.0 运行时只有一个进程，不包含 Tauri、WebView2、Chromium、Node.js、Tokio 或第三方 GUI 框架。

### 运行时职责

```text
┌────────────────────────────────────────────────────────────┐
│ bentodesk-shell — process entry · WndProc                 │
│                   UI-thread dispatch                       │
├────────────────────────────────────────────────────────────┤
│ bentodesk-app — state · interaction · command bus         │
│                  render projection                         │
│ bentodesk-backend — settings · plugins · rules · recovery │
│                      Shell/OLE drag                         │
├────────────────────────────────────────────────────────────┤
│ bentodesk-zone · layout · tree · style · theme · widget   │
│ Domain model · layout · UI tree · tokens · controls       │
├────────────────────────────────────────────────────────────┤
│ bentodesk-platform — low-level Win32 · D2D/DWrite         │
│                      D3D11/DXGI · DComp · WIC              │
└────────────────────────────────────────────────────────────┘
```

> 这是职责图，不是完整的 crate 依赖图。

### 参数

| 口径 | 结果 |
| --- | ---: |
| v2.0.8 便携 ZIP 内的 EXE | **2.41 MiB** |
| v2.0.8 安装程序 | **1.14 MiB** |
| v2.0.8 便携 ZIP | **1.29 MiB** |
| v2.0.2 Private Bytes（t10 / t30 / t60） | **17.80 / 17.38 / 17.34 MiB** |
| v2.0.2 Zone 完整展开 / 收起 | **234 ms / 235 ms** |
| v2.0.2 动画 tick median / p95 | **16 ms / 16 ms** |

这组运行数据只来自一次公开参考测量：2560×1368、144 DPI、5 个 Zone、50 个项目。它不是长期统计，也没有同机同场景的 Tauri 1.x 对照，所以这里只列原始结果，不写“提升了多少倍”。

## 6. 结语

BentoDesk 目前只支持 Windows，也不会替代 Explorer。它只管一件事：让常用文件留在手边，同时别把桌面铺满。

项目采用 [GNU AGPL-3.0-or-later](https://github.com/ZRainbow1275/bentodesk/blob/main/LICENSE) 开源，完全离线，不包含遥测。源码、安装版和便携版都在 [GitHub](https://github.com/ZRainbow1275/bentodesk)。

如果你准备试用，我最想收到高 DPI、多显示器和真实文件拖放方面的反馈。遇到问题直接开 Issue，中文和 English 都可以。

BentoDesk 由方寒维护。

---

## 微信公众号发布版

### 标题候选

1. BentoDesk 2.0：把桌面文件折进 Zone
2. 我把 BentoDesk 重写成了 2.41 MiB 的原生 Windows 应用
3. 文件留在手边，桌面安静一点

### 摘要

BentoDesk 会把普通 Windows 文件折进小小的 Zone：平时是一枚胶囊，需要时原地展开。2.0 移除了网页运行时，自建 Rust 2024 + Win32 / D2D / DWrite / DComp UI 栈；当前 EXE 为 2.41 MiB，公开版 v2.0.2 的一次参考测量在 t60 记录到 17.34 MiB Private Bytes。

### 排版说明

- 直接使用上面的“中文母稿”，但把段落控制在 2–4 行；
- 首屏放 `desktop-tour.webp`，技术重写部分前放 `zone-motion.webp`；
- 微信公众号编辑器不直接渲染 Markdown。发布前转换成内联样式 HTML，或在编辑器中重新排版；
- 外链是否可点击取决于账号与编辑器状态，发布前逐一预览；至少把仓库与 Release 地址保留为可复制文本；
- 不要堆徽章或十几张功能截图，正文保留两段动图和两张参数表即可。

---

## NodeSeek 发布版

### 标题

[开源] BentoDesk 2.0：2.41 MiB 的原生 Rust/Win32 桌面整理器

### 正文

文件藏深会忘，全摊在桌面又乱。BentoDesk 把普通文件、文件夹和快捷方式收进 Zone：平时是一枚胶囊，需要时在原地展开。

1.x 使用 Tauri；2.0 改为原生实现。

**技术栈：** Rust 2024 · Win32 / USER32 / DWM · Direct2D / DirectWrite / DirectComposition · D3D11 / DXGI · WIC · Windows Shell / OLE

单进程，完全离线，无遥测；运行时不包含 Tauri、WebView2、Chromium、Node.js 或 Tokio。

**v2.0.8：** EXE 2.41 MiB · 安装程序 1.14 MiB · 便携 ZIP 1.29 MiB

**v2.0.2 参考测量：** 5 个 Zone / 50 项目 / 144 DPI · t60 Private Bytes 17.34 MiB · 展开/收起 234/235 ms · tick p95 16 ms

两组数字对应不同版本；没有同机同场景的 Tauri 1.x 基准，不做倍数比较。

[演示视频](https://github.com/ZRainbow1275/bentodesk/blob/main/docs/media/desktop-tour.mp4) · [GitHub 仓库](https://github.com/ZRainbow1275/bentodesk) · [下载 2.0.8](https://github.com/ZRainbow1275/bentodesk/releases/tag/v2.0.8)

支持 Windows 10 1809+ / Windows 11。欢迎实测高 DPI、多显示器、动画和文件拖放。

---

## Reddit 发布版

> 发帖前先读目标 subreddit 的最新 sidebar / rules。不要在多个社区短时间复制同一篇正文，也不要请求 upvote。

### r/SideProject 标题

I built a local-first Windows desktop organizer that folds files into small native “Zones”

### r/SideProject 正文

My desktop has always had the same problem: if I file something away, I forget it; if I keep it visible, the desktop turns into a pile.

**BentoDesk** is my attempt at a middle ground. It folds normal Windows files, folders, and shortcuts into small “Zones.” They stay compact when idle, expand in place when needed, and get out of the way again afterwards. The files remain normal Windows files; optional Stealth mode stores them in a local recovery directory.

Version 1 used Tauri. Version 2 is native Rust/Win32: Direct2D, DirectWrite, DirectComposition, D3D11/DXGI, WIC, and Shell/OLE. It runs as one process with no WebView or Node runtime.

The v2.0.8 EXE is 2.41 MiB and the installer is 1.14 MiB. A recorded public v2.0.2 reference run reached 17.34 MiB Private Bytes at t60 and measured 234/235 ms for a full expand/collapse.

Windows 10 1809+ / Windows 11 only. Fully offline, no telemetry, no cloud account.

Demo: https://github.com/ZRainbow1275/bentodesk/blob/main/docs/media/desktop-tour.mp4

Source and download: https://github.com/ZRainbow1275/bentodesk

I’d love to know whether this fits anyone else’s workflow—or what kind of desktop setup breaks it first.

### r/rust：只保留事实，由作者亲自成稿

`r/rust` 当前规则允许有限度的自我推广，但每周最多一次；疑似 AI 生成内容可由版主移除。不要复制上面的英文稿。亲自写时只参考：

- 从 Tauri 1.x 到 Rust 2024 / Win32 原生重写；
- 技术栈：Win32 / USER32 / DWM、Direct2D / DirectWrite / DirectComposition、D3D11 / DXGI、WIC、Shell / OLE；
- 单进程，无 Tauri / WebView2 / Chromium / Node.js / Tokio；
- 拖出 Zone 后，只有 Shell 明确返回 MOVE/COPY 才更新记录；
- v2.0.8：EXE 2.41 MiB、安装程序 1.14 MiB、便携 ZIP 1.29 MiB；
- v2.0.2 参考测量：t60 Private Bytes 17.34 MiB、展开/收起 234/235 ms、tick p95 16 ms；
- `opt-level="z"`、Fat LTO、单 codegen unit、abort / strip / static CRT；
- Windows-only、AGPL-3.0-or-later、无遥测。

### r/opensource：不要使用本稿成文

`r/opensource` 当前规则把所有 AI 生成内容视为可封禁的低质量内容。该社区的最终标题和正文都由你亲自写，并使用 `Promotional` flair；同时确认账号不是只用于推广，仓库许可证仍为 OSI 认可的 AGPL-3.0-or-later。

---

## X 发布版

> 直接上传 `desktop-tour.mp4`，正文只发这一帖。

BentoDesk 2.0 folds normal Windows files into compact Zones that expand in place.

Native Rust/Win32. One process, fully offline, no WebView or Node runtime. v2.0.8: 2.41 MiB EXE, 1.14 MiB installer.

Source + download: https://github.com/ZRainbow1275/bentodesk

[attach desktop-tour.mp4]
