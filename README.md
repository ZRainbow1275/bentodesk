<div align="center">
  <img src="crates/bento-nano-app/assets/app-icon.png" width="112" alt="BentoDesk 图标">

  # BentoDesk

  **由 Rust 驱动，极致优雅的下一代便当盒式 Windows 桌面整理器。**

  [简体中文](README.md) · [English](README.en.md)

  [![Latest release](https://img.shields.io/github/v/release/ZRainbow1275/bentodesk?style=flat-square&label=release)](https://github.com/ZRainbow1275/bentodesk/releases/latest)
  [![Downloads](https://img.shields.io/github/downloads/ZRainbow1275/bentodesk/total?style=flat-square&label=downloads)](https://github.com/ZRainbow1275/bentodesk/releases)
  [![Stars](https://img.shields.io/github/stars/ZRainbow1275/bentodesk?style=flat-square)](https://github.com/ZRainbow1275/bentodesk/stargazers)
  [![CI](https://img.shields.io/github/actions/workflow/status/ZRainbow1275/bentodesk/ci.yml?branch=main&style=flat-square&label=build)](https://github.com/ZRainbow1275/bentodesk/actions/workflows/ci.yml)
  [![Windows](https://img.shields.io/badge/Windows-10%20%2F%2011-0078D4?style=flat-square&logo=windows11&logoColor=white)](#系统要求)
  [![Rust](https://img.shields.io/badge/Rust-2024-000000?style=flat-square&logo=rust&logoColor=white)](#技术栈)
  [![License](https://img.shields.io/badge/License-AGPL--3.0-22c55e?style=flat-square)](LICENSE)

  [下载](https://github.com/ZRainbow1275/bentodesk/releases/latest) ·
  [功能](#功能) ·
  [使用](#快速开始) ·
  [构建](#从源码构建) ·
  [贡献](#参与贡献)
</div>

<!-- MEDIA: docs/media/hero.webp | 16:9 | BentoDesk 2.0 desktop overview and Zone motion. Replace this comment only with current native UI media supplied by the maintainer. -->

## 为什么做 BentoDesk

我是那种喜欢将所有东西摆放在桌面上的人，为此将桌面从C盘移到了D盘。随着工作变多，桌面逐渐变得像个垃圾堆。

BentoDesk 是为了解决我的问题被开发出来的软件。它在桌面上提供一组可以收起、展开和组合的 Zone。并像便当盒一样把不同内容分开放好：平时只保留克制的胶囊，需要时再展开。文件仍由本机 Windows 文件系统管理，BentoDesk 负责组织、呈现和恢复。

2.0 使用 Rust 与 Windows 原生图形栈重新实现。在 1.0 的功能设计基础上，完善开发问题，实现最极致的效果。

## 四个特点

| | |
| --- | --- |
| **极致** | 单进程原生运行时；Release EXE 约 2.40 MiB，严格五 Zone 场景下 Private Bytes 保持在 18 MiB 以内。 |
| **优雅** | Zone、Stack、主题、文字和动画共用一套几何与状态，不再依靠网页图层拼接桌面界面。 |
| **便捷** | 拖放、搜索、堆叠、批量排列、快照与时间线都直接作用于真实桌面内容。 |
| **安全** | 数据默认留在本机；设置可使用 DPAPI 或口令加密，插件安装、启停与卸载经过路径和清单校验。 |

## 与常见方案的差别

| 方案 | 桌面上一眼可见 | 运行时 | 本地数据 | 扩展与整理 |
| --- | --- | --- | --- | --- |
| Windows 文件夹 | 打开后可见 | Explorer | 本地 | 文件夹与系统搜索 |
| 桌面围栏类产品 | 可见 | 各产品不同 | 各产品不同 | 以围栏和布局为主 |
| Electron / WebView 整理工具 | 可见 | 浏览器内核 + 应用层 | 取决于产品 | Web 技术生态 |
| **BentoDesk 2.0** | 胶囊常驻，按需展开 | Rust + Win32 + DirectComposition | 本地、可加密 | Zone、Stack、插件、规则、快照与时间线 |

BentoDesk 不替代 Explorer，也不承诺适合所有工作流。它专注于一个问题：让需要留在桌面上的内容既能被看见，又不会一直占满屏幕。

## 功能

### Zone：收起时安静，展开时完整

Zone 支持不同宽度、胶囊尺寸和边角；标题、图标、项目数徽标与展开内容使用同一套布局。可以选择悬停展开、单击展开或常驻展开。

<!-- MEDIA: docs/media/zone-motion.webp | 16:9 | One Zone showing collapsed, expand, rapid reversal and settled expanded states. -->

### 拖放、堆叠与桌面融合

支持 Windows Shell/OLE 拖放、Zone 间移动与复制、拖出恢复、文件夹绑定和 Stack。桌面空白区域保持点击穿透；普通应用窗口能够正常遮挡 BentoDesk。

<!-- MEDIA: docs/media/drag-stack.webp | 16:9 | Real file drag into/out of a Zone and two Zones forming a Stack. -->

### 搜索、编辑与批量管理

Zone 内搜索只过滤当前 Zone；全局搜索用于跨 Zone 查找。区域编辑器可以修改名称、别名、图标、强调色、列数、宽度与边角。批量管理支持选择、显示、隐藏、移动、删除和五种布局。

<!-- MEDIA: docs/media/search-bulk.webp | 16:9 | Local Zone search followed by the native bulk manager. -->

### 设置、主题与中英双语

设置窗口是原生、可拖动、可滚动且非置顶的普通窗口。内置明暗主题、强调色、性能参数、启动选项、备份、加密与更新设置。首版默认简体中文，可在 Settings 中切换为 English。

<!-- MEDIA: docs/media/settings-themes.webp | 16:9 | Settings Appearance page switching between verified light and dark themes. -->

### 智能分组、插件与实时文件夹

智能分组建议根据真实桌面文件生成可审阅方案；插件支持本地包安装、启停、持久化与确认卸载；实时文件夹可把受监视目录的变化同步到对应 Zone。

<!-- MEDIA: docs/media/smart-group-plugins.webp | 16:9 | Smart grouping review and plugin management using current native surfaces. -->

### 快照、时间线与恢复

可以保存和载入布局快照，通过时间线恢复结构变化，并从托盘、全局快捷键或原生辅助窗口进入管理工具。恢复包和更新器沿用本地校验边界，不启动浏览器内核或额外应用进程。

<!-- MEDIA: docs/media/snapshots-timeline.webp | 16:9 | Layout snapshot and timeline recovery flow. -->

## 快速开始

1. 从 [Releases](https://github.com/ZRainbow1275/bentodesk/releases/latest) 下载 `BentoDesk-2.0.0-windows-x64-portable.zip`。
2. 解压到普通可写目录，运行 `BentoDesk.exe`。
3. 从托盘菜单新建或管理 Zone。
4. 把文件、文件夹或快捷方式拖入 Zone。
5. 在设置中选择主题、展开模式与语言。

便携包无需 Node.js、WebView2 或额外浏览器内核。程序状态默认保存在当前用户目录；如需随程序目录携带，可在设置中启用便携模式。

### 系统要求

- Windows 10 1809+ 或 Windows 11；
- x86-64 处理器；
- 建议使用当前 Windows 更新与显卡驱动。

## 当前候选实测

以下数据来自同一台 Windows x64 机器上的隔离场景，只描述本次候选，不代表所有硬件：

| 指标 | 结果 |
| --- | ---: |
| Release EXE | 2,518,528 bytes（2.40 MiB） |
| 严格场景 | 5 Zones / 50 items / 1 process |
| Private Bytes t30 | 17.43 MiB |
| Private Bytes t60 | 17.33 MiB |
| Zone 完整展开 / 收起 | 234 ms / 234 ms |
| 帧间隔 median / p95 | 16 ms / 16 ms |

## 技术栈

| 层 | 实现 |
| --- | --- |
| 语言与运行时 | Rust 2024，单进程 |
| 窗口与输入 | Win32 / USER32 / DWM |
| 图形 | Direct2D、DirectWrite、DirectComposition、D3D11 |
| 图标与图像 | Windows Imaging Component、Windows Shell |
| 文件交互 | Shell/OLE、ReadDirectoryChangesW |
| 网络与系统安全 | WinHTTP、DPAPI |
| 数据 | 本地原子写入、加密设置仓库 |
| 构建 | MSVC x64、静态 CRT、size optimization、Fat LTO |

主要 crate：

```text
bento-nano-shell      进程入口、Win32 消息路由与系统集成
bento-nano-app        应用状态、交互、渲染投影
bento-nano-backend    设置、插件、规则、恢复与更新
bento-nano-platform   D2D/DWrite/DComp/WIC/Shell/OLE 边界
bento-nano-zone       Zone 领域模型
bento-nano-style      主题、排版与视觉 token
```

## 从源码构建

需要：

- Windows 10/11 x64；
- Rust 1.85 或更高版本；
- Visual Studio 2022 Build Tools（MSVC 与 Windows SDK）。

```powershell
git clone https://github.com/ZRainbow1275/bentodesk.git
cd bentodesk

$env:CARGO_BUILD_JOBS = "1"
$env:CARGO_INCREMENTAL = "0"
cargo build --release --target x86_64-pc-windows-msvc -p bento-nano-shell --bin BentoDesk
```

输出：

```text
target\x86_64-pc-windows-msvc\release\BentoDesk.exe
```

完整质量检查：

```powershell
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
cargo deny check
cargo audit
```

## 参与贡献

欢迎代码、测试、翻译、主题、插件和文档贡献。Issue 与 Pull Request 可以使用中文或 English。

提交前请至少运行与改动相关的测试，并说明真实验证边界。涉及文件移动、插件、更新或恢复逻辑时，请同时说明数据完整性与回滚方式。

## 致谢

感谢 GPT 5.6 Sol 在工程重塑和验证中的协作，感谢 [Tibo](https://x.com/thsottiaux) 带来的产品启发，也感谢 Linux Do 社区长期提供的讨论、测试与反馈。

BentoDesk 由方寒（[@ZRainbow1275](https://github.com/ZRainbow1275)）维护。

## 许可证

BentoDesk 以 [GNU AGPL-3.0-or-later](LICENSE) 发布。使用、修改或分发本项目时，请遵守许可证要求。
