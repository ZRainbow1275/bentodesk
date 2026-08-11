# BentoDesk 社区发布初稿

> 状态：初稿，2026-08-12。
> 用途：微信公众号、NodeSeek、Reddit、X 等平台的发布素材。
> 重要：**不要把本文原样发布到 Hacker News 或 Linux DO。** HN 的现行发布建议要求文字由作者亲手写；Linux DO 不允许直接发布 AI 生成或润色文字。文末只提供事实提纲与合规清单。



# BentoDesk：优雅、极致、安全，由rust驱动的下一代windows桌面管理工具

这个标题可能有点大哈233333我先检讨一下。

我一直是从桌面开始工作的人，我喜欢用文件夹和区域分出四象限和轻重缓急，来安排事情，一打开电脑满满的秩序感和苦逼工作感就很安心。

（说老实话期间我想转去使用mac，但是mac的性能啊....还有开发上的限制，导致我捏着鼻子也要继续用windows...）

随着事情变多，麻烦就来了。什么东西都出现在桌面上，这个时候就需要进一步排序。

我想过文件夹套娃，但是效果不好，不够优雅关键是。特别是一个项目套一个项目，翻来翻去的很烦，甚至最后自己都忘记打开。

所以，项目本身只是为了解决我的痛点需求，要一个优雅的解决方案。

**目的是让我进一步归类文件，但是对电脑内存的占用还不要多，特别能练习rust就更好了。**

于是我做了 BentoDesk。

BentoDesk 是一个优雅、极致、安全，由rust驱动的下一代windows桌面管理工具。

它把普通文件、文件夹和快捷方式放进一个个 Zone。Zone 平时收成一枚小胶囊，需要时就在原地展开成网格；使用结束后，再把空间还给桌面。

几个相关的 Zone 还可以组成一个 Stack。展开 Stack 时，各个 Zone 会在桌面上铺开，收起后又只剩下一个入口。

文件没有被导入某个专有资料库，也没有被锁进 BentoDesk 自己的文件系统。它们始终是普通的 Windows 文件。拖放交给 Windows Shell/OLE，只有系统确认操作完成后才更新 Zone；Stealth 模式会把文件移到本地恢复目录。重要文件还是应该正常备份。

够轻便、不打扰、够安心，就是我想要 BentoDesk 实现的。

---

## 功能

- **Zone：** 胶囊和展开网格共用同一块原生界面。名称、别名、图标、颜色、列数、宽度和五种边角都能单独调整；展开方式可选悬浮、单击或常驻。
- **文件拖放：** 使用 Windows Shell/OLE 处理文件、文件夹和快捷方式。轻松便捷地将文件在Zone内拽入拽出。
- **搜索与 Stack：** 可以搜索当前 Zone，也可以搜索全部 Zone。多个 Zone 能组合成 Stack，各自的布局和样式不会被抹掉。
- **批量整理：** 可以同时显示、隐藏、移动或删除多个 Zone，并按网格、横向、纵向、环绕或自然方式重新排列。
- **自动整理：** 智能分组只生成建议，由用户决定是否应用；Live Folder 和规则处理重复整理。
- **个性化：**尽力支持了一套本地主题插件系统，尽力为自定义开放大门。
- **恢复与设置：** 支持布局快照、时间线、设置备份和加密。批量删除、快照删除等操作需要确认。
- **外观与语言：** 支持多个*炫酷*主题、强调色、动画和性能选项；支持双语。

![BentoDesk 桌面演示](https://raw.githubusercontent.com/ZRainbow1275/bentodesk/main/docs/media/desktop-tour.webp)

![Zone 展开与搜索](https://raw.githubusercontent.com/ZRainbow1275/bentodesk/main/docs/media/zone-motion.webp)

---

## 技术栈

| 项目             | 技术                                                |
| ---------------- | --------------------------------------------------- |
| 语言             | Rust 2024                                           |
| Windows 与输入   | Win32 / USER32 / DWM                                |
| 图形与文字       | Direct2D / DirectWrite / DirectComposition          |
| GPU 与交换链     | D3D11 / DXGI                                        |
| 图标、图片与文件 | WIC / Windows Shell / OLE / `ReadDirectoryChangesW` |
| 本地数据         | DPAPI / 原子写入 / 加密设置 vault                   |
| 构建             | MSVC x64 / static CRT / `opt-level="z"` / Fat LTO   |

---

## 特色

Fences 和 Portals 已经把桌面整理做得很成熟。BentoDesk 没打算照着它们补一张更长的功能表，而是选了一个更窄的方向：不用时尽量缩小，需要时就在原处打开。

| 方案                                                         | 更适合的情况                                                 |
| ------------------------------------------------------------ | ------------------------------------------------------------ |
| Windows 文件夹和桌面图标                                     | 不想安装额外软件，只需要系统原有的文件夹与快捷方式。         |
| [Stardock Fences 6](https://www.stardock.com/products/fences/) | 看重成熟的自动分类规则、Peek、Folder Portals、标签页或企业部署。 |
| [Portals](https://portals-app.com/)                          | 需要常驻文件夹面板、多标签、细致的面板样式和随显示器切换的布局。 |
| **BentoDesk**                                                | 希望整理器平时尽量缩小；需要 Zone 原地展开、Stack、直接桌面文件操作、布局快照和时间线恢复；偏好开源、离线和原生 Windows 实现。 |

**v2.0.2 参考构建 EXE：2.39 MiB**

**v2.0.2 单次参考测量 Private Bytes（t60）：17.34 MiB**

**全程序只有一个进程**

---

## 安装

**GitHub：** [ZRainbow1275/bentodesk](https://github.com/ZRainbow1275/bentodesk)

**下载最新版：** [GitHub Releases](https://github.com/ZRainbow1275/bentodesk/releases/latest)

支持 Windows 10 1809+ 和 Windows 11，仅提供 x86-64 版本。

有安装版和便携版区分。

---

## 最后

项目其实是2.0，因为1.0虽然也有rust，但是前端是ty的壳，在性能上不够优雅，因此换了技术栈才敢发布出来。

在期间肯定是和AI协同工作的，我的专业毕竟是法律，还要慢慢学习编程。欢迎大佬捉虫指教，我会尽力维护。

项目作者一直在用，但目前只有少数用例，欢迎大家提交issues。

代码以 AGPL-3.0-or-later 开源。Issue 和 Pull Request 都可以使用中文或英文，主题、插件、文档、翻译和明确的 Bug 复现都欢迎。



喜欢的话给个star吧！



---

## 微信公众号发布版

### 标题候选

1. BentoDesk 2.0：把桌面文件折进 Zone
2. 我把 BentoDesk 重写成了原生 Windows 应用
3. 文件留在手边，桌面安静一点

### 摘要

BentoDesk 会把普通 Windows 文件折进小小的 Zone：平时是一枚胶囊，需要时原地展开。2.0 移除了网页运行时，自建 Rust 2024 + Win32 / D2D / DWrite / DComp UI 栈；v2.0.8 公开构建的 EXE 为 2.41 MiB，v2.0.2 的一次参考测量在 t60 记录到 17.34 MiB Private Bytes。

### 排版说明

- 直接使用上面的“中文母稿”，但把段落控制在 2–4 行；
- 首屏放 `desktop-tour.webp`，技术重写部分前放 `zone-motion.webp`；
- 微信公众号编辑器不直接渲染 Markdown。发布前转换成内联样式 HTML，或在编辑器中重新排版；
- 外链是否可点击取决于账号与编辑器状态，发布前逐一预览；至少把仓库与 Release 地址保留为可复制文本；
- 不要堆徽章或十几张功能截图，正文保留两段动图和两张参数表即可。

---

## NodeSeek 发布版

### 标题

[开源] BentoDesk 2.0：原生 Rust/Win32 桌面整理器

### 正文

文件藏深会忘，全摊在桌面又乱。BentoDesk 把普通文件、文件夹和快捷方式收进 Zone：平时是一枚胶囊，需要时在原地展开。

1.x 使用 Tauri；2.0 改为原生实现。

**技术栈：** Rust 2024 · Win32 / USER32 / DWM · Direct2D / DirectWrite / DirectComposition · D3D11 / DXGI · WIC · Windows Shell / OLE

单进程，完全离线，无遥测；运行时不包含 Tauri、WebView2、Chromium、Node.js 或 Tokio。

**v2.0.8 参考构建：** EXE 2.41 MiB · 安装程序 1.14 MiB · 便携 ZIP 1.29 MiB

**v2.0.2 参考测量：** 5 个 Zone / 50 项目 / 144 DPI · t60 Private Bytes 17.34 MiB · 展开/收起 234/235 ms · tick p95 16 ms

两组数字对应不同版本；没有同机同场景的 Tauri 1.x 基准，不做倍数比较。

[演示视频](https://github.com/ZRainbow1275/bentodesk/blob/main/docs/media/desktop-tour.mp4) · [GitHub 仓库](https://github.com/ZRainbow1275/bentodesk) · [下载最新版](https://github.com/ZRainbow1275/bentodesk/releases/latest)

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

The reference v2.0.8 EXE is 2.41 MiB and its installer is 1.14 MiB. A recorded public v2.0.2 reference run reached 17.34 MiB Private Bytes at t60 and measured 234/235 ms for a full expand/collapse.

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
- v2.0.8 参考构建：EXE 2.41 MiB、安装程序 1.14 MiB、便携 ZIP 1.29 MiB；
- v2.0.2 参考测量：t60 Private Bytes 17.34 MiB、展开/收起 234/235 ms、tick p95 16 ms；
- `opt-level="z"`、Fat LTO、单 codegen unit、abort / strip / static CRT；
- Windows-only、AGPL-3.0-or-later、无遥测。

### r/opensource：不要使用本稿成文

`r/opensource` 当前规则把所有 AI 生成内容视为可封禁的低质量内容。该社区的最终标题和正文都由你亲自写，并使用 `Promotional` flair；同时确认账号不是只用于推广，仓库许可证仍为 OSI 认可的 AGPL-3.0-or-later。

---

## X 发布版

> 直接上传 `desktop-tour.mp4`，正文只发这一帖。

BentoDesk 2.0 folds normal Windows files into compact Zones that expand in place.

Native Rust/Win32. One process, fully offline, no WebView or Node runtime. Reference v2.0.8 build: 2.41 MiB EXE, 1.14 MiB installer.

Source + download: https://github.com/ZRainbow1275/bentodesk

[attach desktop-tour.mp4]
