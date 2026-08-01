<div align="center">
  <img src="crates/bentodesk-app/assets/app-icon.png" width="96" alt="BentoDesk 图标">

  # BentoDesk

  **由 Rust 驱动，极致优雅的下一代便当盒式 Windows 桌面整理器。**

  [English](README.md) · [简体中文](README.zh-CN.md)

  <p><a href="https://github.com/ZRainbow1275/bentodesk/releases/latest"><img src="https://img.shields.io/github/v/release/ZRainbow1275/bentodesk?style=flat-square&amp;label=release" alt="最新版本"></a> <a href="https://github.com/ZRainbow1275/bentodesk/releases"><img src="https://img.shields.io/github/downloads/ZRainbow1275/bentodesk/total?style=flat-square&amp;label=downloads" alt="总下载量"></a> <a href="https://github.com/ZRainbow1275/bentodesk/stargazers"><img src="https://img.shields.io/github/stars/ZRainbow1275/bentodesk?style=flat-square&amp;label=stars" alt="GitHub Stars"></a> <a href="https://github.com/ZRainbow1275/bentodesk/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/ZRainbow1275/bentodesk/ci.yml?branch=main&amp;style=flat-square&amp;label=build" alt="CI"></a> <a href="#系统要求"><img src="https://img.shields.io/badge/Windows-10%20%2F%2011-0078D4?style=flat-square&amp;logo=windows11&amp;logoColor=white" alt="Windows 10 与 11"></a> <a href="Cargo.toml"><img src="https://img.shields.io/badge/Rust-2024-000000?style=flat-square&amp;logo=rust&amp;logoColor=white" alt="Rust 2024"></a> <a href="LICENSE"><img src="https://img.shields.io/badge/License-AGPL--3.0-22c55e?style=flat-square" alt="AGPL-3.0 许可证"></a></p>

  <p>
    <a href="https://github.com/ZRainbow1275/bentodesk/releases/latest"><strong>下载</strong></a> ·
    <a href="docs/media/desktop-tour.mp4">观看完整演示</a> ·
    <a href="#动画就是界面">看看怎么用</a> ·
    <a href="#从源码构建">从源码构建</a> ·
    <a href="#参与贡献">参与贡献</a>
  </p>
</div>

<p align="center">
  <a href="docs/media/desktop-tour.mp4"><img src="docs/media/desktop-tour.webp" width="560" alt="BentoDesk 真实桌面演示：Zone 展开、Stack 绽放，最后重新收回桌面"></a>
</p>
<p align="center">
  <sub>Zone 展开，Stack 绽放，然后桌面重新安静下来。 · <a href="docs/media/desktop-tour.mp4">播放 33 秒 MP4</a></sub>
</p>

## 文件在手边，噪音收起来

我习惯从桌面开始工作。文件很有用，堆成一片就不是了。

BentoDesk 把普通 Windows 文件折叠进小小的 **Zone**。Zone 平时收成胶囊，
需要时就在原地展开成网格，离开后再把空间还给桌面。相关的 Zone 可以组成
**Stack**，但仍保留各自的布局和样式。文件不会被导入某种专有资料库，它们
依旧是普通的 Windows 文件。

<table width="100%">
  <tr>
    <td width="33%" align="center"><strong>停靠</strong><br><sub>一枚小胶囊，不抢桌面。</sub></td>
    <td width="33%" align="center"><strong>展开</strong><br><sub>在原地搜索、打开与整理。</sub></td>
    <td width="33%" align="center"><strong>收回</strong><br><sub>离开以后，把空间还回来。</sub></td>
  </tr>
</table>

2.0 是 BentoDesk 的原生 Rust 重写。Tauri 1.x 中实用的想法保留下来，网页
运行时被移除；动画、排版、文件处理、设置与系统集成都按 Windows 桌面软件
重新完成。

## 动画就是界面

胶囊和网格是同一个原生表面。几何、命中、文字、图标与控件共用一套动画
状态，因此反向操作会从当前画面继续，而不是突然切到另一层。

<p align="center">
  <a href="docs/media/zone-motion.mp4"><img src="docs/media/zone-motion.webp" width="500" alt="BentoDesk Zone 的真实展开、搜索与原生右键菜单动画"></a>
</p>
<p align="center">
  <sub>胶囊 → 网格 → 搜索 → 右键菜单 · <a href="docs/media/zone-motion.mp4">播放 MP4</a></sub>
</p>

Zone 可以选择宽度和五种边角，也可以悬停展开、单击展开或常驻展开。快速
滑过多个 Zone 时，尚未结束的动画不会把后续动画吞掉。

## 桌面的其他部分

### 01 · 把一个 Zone 调到顺手

名称、别名、图标、强调色、网格列数、宽度与边角都能直接编辑，不会在桌面上
弹出依赖浏览器内核的设置窗口。

<p align="center">
  <img src="docs/images/zone-editor.png" width="560" alt="BentoDesk 原生 Zone 编辑器">
</p>

### 02 · 一次整理整张桌面

批量选择、显示、隐藏、移动或删除 Zone。网格、横排、纵列、环绕与自然布局
都会限制在显示器的可用范围内。

<p align="center">
  <img src="docs/images/bulk-manager.png" width="720" alt="BentoDesk 原生批量区域管理器">
</p>

### 03 · 让它融进自己的桌面

明暗主题、强调色、展开方式、性能参数、启动选项、备份、加密、插件与更新都
放在原生设置中。首次运行跟随 Windows 界面语言，之后可随时切换中文和
English。

<p align="center">
  <img src="docs/images/theme-settings.png" width="420" alt="BentoDesk 原生主题选择器">
</p>

## 该小的地方，真的很小

<div align="center">
<table>
  <tr>
    <td width="25%" align="center"><strong>2.39 MiB</strong><br><sub>Release 可执行文件</sub></td>
    <td width="25%" align="center"><strong>17.34 MiB</strong><br><sub>t60 Private Bytes</sub></td>
    <td width="25%" align="center"><strong>235 ms</strong><br><sub>Zone 完整收起</sub></td>
    <td width="25%" align="center"><strong>1 个进程</strong><br><sub>不携带浏览器运行时</sub></td>
  </tr>
</table>
</div>

这些数字来自一次隔离的五 Zone 实测，不是合成跑分。完整场景和边界见
[参考实测](#参考实测)。

## BentoDesk 能处理什么

### 日常操作

- 通过 Windows Shell/OLE 移动或复制文件、文件夹与快捷方式，也可以把项目
  从 Zone 拖回桌面；
- 在一个 Zone 内筛选，或跨全部 Zone 搜索；
- 把多个 Zone 组成 Stack，再展开成员，同时保留各自布局；
- 桌面空白区域保持点击穿透，普通应用窗口可以正常覆盖 BentoDesk。

### 自动化，但先让人看一眼

- **智能分组**根据桌面文件生成可选建议，只有应用建议后才会移动内容；
- **实时文件夹**让绑定目录与对应 Zone 保持同步；
- **插件**从通过校验的本地包安装，支持启停、持久化与确认卸载；
- **规则**处理重复的本地整理，不依赖在线服务。

### 恢复与文件安全

可以保存和载入布局快照，从时间线查看结构变化并恢复旧布局。设置备份、原子
持久化、更新包校验，以及 DPAPI 或口令加密提供恢复路径，不需要辅助浏览器
进程。

拖放遵循 Windows 的移动与复制语义。失败或被拒绝的传输不会被记录为完成。
删除快照和批量删除 Zone 需要确认。不可替代的文件仍应像使用其他文件工具时
一样保留备份。

## 如何选择桌面整理器

下面这些软件解决的是不同版本的“桌面放满了”。对照内容来自各产品的官方
说明，只比较工作方式，不拿合成跑分代替真实体验。

| 可以先看 | 它更适合什么情况 |
| --- | --- |
| [Windows 文件夹与桌面图标](https://support.microsoft.com/zh-cn/windows/experience/personalization/customize-the-desktop-icons-in-windows) | 不想安装额外软件，继续使用熟悉的图标、快捷方式与 Explorer 文件夹。 |
| [Stardock Fences 6](https://www.stardock.com/products/fences/) | 看重丰富自动整理或受管电脑部署，包括排序规则、标签页、Peek、Folder Portal 与企业控制。 |
| [Portals](https://portals-app.com/) | 想要常驻文件夹面板、标签页、逐面板样式与随显示器切换的档案。 |
| [Nimi Places](https://mynimi.net/Projects/Nimi-Places/Features/) | 最看重预览、标签、排序规则与偏媒体浏览的容器。 |
| **BentoDesk** | 想要本地优先，而且静止时尽量少占桌面的整理器：原生胶囊到网格动画、Stack、真实桌面项目、布局、快照与时间线恢复。 |

BentoDesk 刻意保持专注：只支持 Windows，不替代 Explorer，也不提供云账号
同步。常驻文件夹门户、丰富媒体浏览或企业受管部署，应交给专门为它们设计的
产品。

## 快速开始

1. 从 [Releases](https://github.com/ZRainbow1275/bentodesk/releases/latest)
   下载 `BentoDesk-2.0.3-windows-x64-portable.zip`；
2. 使用同页 `SHA256SUMS.txt` 校验压缩包，解压到普通可写目录，运行
   `BentoDesk.exe`；
3. 从托盘菜单新建或管理 Zone；
4. 把文件、文件夹或快捷方式拖入 Zone；
5. 在设置中选择主题、展开方式与语言。

便携包不需要 Node.js、Tauri、WebView2 或额外浏览器运行时。程序状态默认
保存在当前用户目录；启用便携模式后，也可以随程序目录携带。

### 系统要求

- Windows 10 1809+ 或 Windows 11；
- x86-64 处理器；
- 建议使用当前 Windows 更新与显卡驱动。

## 参考实测

一次隔离的公开版 BentoDesk 2.0.2 Windows x64 运行，分辨率 2560×1368、
144 DPI，场景为五个 Zone、50 个项目与一个 BentoDesk 进程：

| 指标 | 结果 |
| --- | ---: |
| Release EXE | 2,506,752 bytes（2.39 MiB） |
| Private Bytes t10 / t30 / t60 | 17.80 / 17.38 / 17.34 MiB |
| Zone 完整展开 / 收起 | 234 ms / 235 ms |
| 动画 tick median / p95 | 16 ms / 16 ms |

## 技术栈

| 层 | 实现 |
| --- | --- |
| 语言与运行时 | Rust 2024，单进程 |
| 窗口与输入 | Win32 / USER32 / DWM |
| 图形 | Direct2D、DirectWrite、DirectComposition、D3D11 |
| 图标与图像 | Windows Imaging Component、Windows Shell |
| 文件交互 | Shell/OLE、`ReadDirectoryChangesW` |
| 网络与系统安全 | WinHTTP、DPAPI |
| 数据 | 本地原子写入、加密设置仓库 |
| 构建 | MSVC x64、静态 CRT、size optimization、Fat LTO |

> [!NOTE]
> BentoDesk 2.0 的运行时不包含 Tauri、WebView2、Chromium、Node.js 或第三方
> GUI 框架。Tauri 1.x 源码仍保留在
> [`v1.3.0`](https://github.com/ZRainbow1275/bentodesk/tree/v1.3.0) 与
> [`archive/tauri-1.x`](https://github.com/ZRainbow1275/bentodesk/tree/archive/tauri-1.x)。

主要 crate：

```text
bentodesk-shell      进程入口、Win32 消息路由与系统集成
bentodesk-app        应用状态、交互、渲染投影
bentodesk-backend    设置、插件、规则、恢复与更新
bentodesk-platform   D2D/DWrite/DComp/WIC/Shell/OLE 边界
bentodesk-zone       Zone 领域模型
bentodesk-style      主题、排版与视觉 token
```

## 从源码构建

需要 Windows 10/11 x64、Rust 1.89 或更高版本，以及包含 MSVC 与 Windows SDK
的 Visual Studio 2022 Build Tools。

```powershell
git clone https://github.com/ZRainbow1275/bentodesk.git
cd bentodesk

$env:CARGO_BUILD_JOBS = "1"
$env:CARGO_INCREMENTAL = "0"
cargo build --locked --release --target x86_64-pc-windows-msvc -p bentodesk-shell --bin BentoDesk
```

输出：

```text
target\x86_64-pc-windows-msvc\release\BentoDesk.exe
```

完整质量检查：

```powershell
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo doc --locked --workspace --no-deps
cargo deny check
cargo audit
```

## 参与贡献

欢迎代码、测试、翻译、主题、插件、文档和边界清楚的 Bug 报告。Issue 与 Pull
Request 可以使用中文或 English；提交前请阅读
[CONTRIBUTING.md](CONTRIBUTING.md)。

安全漏洞请按 [SECURITY.md](SECURITY.md) 使用 GitHub 私密报告，不要公开提交
Issue。

## 致谢

感谢GPT 5.6 SOL的帮助，以及[Tibo](https://x.com/thsottiaux)多次频繁的reset，
大大加快了本项目的面世和完善。也感谢
[Linux Do](https://linux.do/) 社区一路以来的讨论、测试与直率反馈。

<p align="center">
  <img src="docs/media/tibo-reset.webp" width="160" alt="Tibo reset 搞怪图">
</p>

BentoDesk 由方寒（[@ZRainbow1275](https://github.com/ZRainbow1275)）维护。

## 许可证

BentoDesk 以 [GNU AGPL-3.0-or-later](LICENSE) 发布。
