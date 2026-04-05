<p align="center">
  <img src="./bentodesk.svg" width="128" height="128" alt="BentoDesk Logo">
</p>

<h1 align="center">BentoDesk</h1>

<p align="center">
  <strong>便当盒式桌面整理器</strong><br>
  <sub>把杂乱的桌面，变成一个优雅的便当盒</sub>
</p>

<p align="center">
  <a href="https://github.com/ZRainbow1275/bentodesk/releases/latest"><img src="https://img.shields.io/github/v/release/ZRainbow1275/bentodesk?style=flat-square&color=DD2476&label=%E4%B8%8B%E8%BD%BD" alt="下载"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/%E5%8D%8F%E8%AE%AE-AGPL--3.0-3b82f6?style=flat-square" alt="AGPL-3.0"></a>
  <img src="https://img.shields.io/badge/%E5%B9%B3%E5%8F%B0-Windows%2010%20%2F%2011-0078D6?style=flat-square" alt="Windows">
  <img src="https://img.shields.io/badge/Tauri-v2-FFC131?style=flat-square" alt="Tauri v2">
  <a href="https://github.com/ZRainbow1275/bentodesk/actions"><img src="https://img.shields.io/github/actions/workflow/status/ZRainbow1275/bentodesk/release.yml?style=flat-square&label=%E6%9E%84%E5%BB%BA" alt="构建状态"></a>
</p>

---

## 为什么做 BentoDesk？

> *桌面上的东西太多太复杂，但它们确实需要出现在眼前，提醒我去行动。*

每个 Windows 用户都遇到过这样的困境——桌面图标越积越多，文件、项目、快捷方式铺满整个屏幕，找东西要翻半天。想整理，但又不想把文件藏进层层文件夹里，因为**看不见就会忘记**。

BentoDesk 就是为了解决这个问题而诞生的。它不改变你的文件路径，不替代文件管理器，只做一件事：**在桌面上创建毛玻璃质感的收纳区域，像便当盒一样优雅地整理你的文件**。鼠标悬停时展开、离开时收起，零干扰，始终在你需要时出现。

## 功能亮点

### 便当区域

- 在桌面上创建可拖拽、可调整大小的毛玻璃收纳区域
- 从资源管理器或其他区域直接拖放文件
- 鼠标悬停自动展开，离开自动收起——零干扰工作流
- 收纳区域支持胶囊形态（药丸、圆角、圆形、极简）与多种尺寸

### 智能整理

- 基于文件类型、名称模式、修改时间的智能分组建议
- 自定义规则，新文件自动归入对应区域
- 布局快照——一键保存与还原整个桌面布局
- 桌面图标位置备份，切换分辨率后不再错乱

### 10 套内置主题

| 深色 Dark | 浅色 Light | 午夜 Midnight | 森林 Forest | 日落 Sunset |
|:---------:|:----------:|:-------------:|:-----------:|:-----------:|
| 磨砂 Frosted | 实色 Solid | 秩序 Order | 新拟态 Neo | 扁平 Flat |

支持完整的自定义主题系统——27 个 CSS 变量全面控制表面、边框、文字、阴影、模糊与圆角。可导入导出 JSON 主题文件。

### 桌面深度融合

- **幽灵图层** —— 透明覆盖层浮于壁纸之上、应用窗口之下，不影响 Alt-Tab
- **穿透点击** —— 非区域部分的点击直接传递到桌面图标
- **原生拖放** —— 基于 Win32 OLE 的文件拖放，与系统完全一致
- **系统托盘** —— 最小化到托盘，右键菜单快速操作
- **文件监视** —— 实时检测桌面新增文件

### 国际化

- 内置简体中文与英文
- 实时切换语言，无需重启

---

## 下载安装

### 直接下载

前往 [**Releases 页面**](https://github.com/ZRainbow1275/bentodesk/releases/latest) 下载最新版本：

| 格式 | 说明 |
|------|------|
| `BentoDesk_x.x.x_x64-setup.exe` | NSIS 安装包（推荐） |
| `BentoDesk_x.x.x_x64_en-US.msi` | MSI 安装包 |

### 系统要求

- **Windows 10**（1809+）或 **Windows 11**
- **WebView2 运行时** —— Windows 11 已内置；Windows 10 用户可能需要 [下载安装](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)

---

## 使用指南

### 快速上手

1. 安装后启动 BentoDesk，桌面上会出现一个透明覆盖层
2. 右键系统托盘图标 → **新建区域**，在桌面上创建你的第一个便当区域
3. 将文件拖入区域，或在资源管理器中拖放到区域上
4. 鼠标悬停到收起的区域上，自动展开查看内容

### 设置项

通过系统托盘图标或界面齿轮按钮打开设置：

| 设置 | 说明 |
|------|------|
| 桌面嵌入层 | 启用/禁用幽灵覆盖层 |
| 开机自启 | 跟随 Windows 启动 |
| 任务栏显示 | 控制是否在任务栏显示 |
| 智能分组 | 启用文件自动分组建议 |
| 便携模式 | 数据存储在程序同目录（需重启） |
| 主题 | 10 套内置主题 + 自定义主题 |
| 展开/收起延迟 | 自定义悬停响应速度 |
| 语言 | 简体中文 / English |

---

## 文件安全说明

BentoDesk 通过将文件移动到桌面下的隐藏文件夹 `.bentodesk/` 来实现区域收纳：

- 文件只在桌面范围内**移动**，**绝不删除**
- 退出时自动将所有文件**还原**到桌面原始位置
- 安全清单 `manifest.json` 记录每个文件的原始路径，即使异常崩溃也可恢复
- 桌面图标位置在启动时备份，退出时还原

> 如果 BentoDesk 异常退出，你的文件安全地保存在 `桌面/.bentodesk/` 中。打开资源管理器显示隐藏文件即可找到。

---

## 从源码构建

<details>
<summary>点击展开构建说明</summary>

### 环境要求

- [Rust](https://rustup.rs/)（最新 stable）
- [Node.js](https://nodejs.org/)（v18+）
- [pnpm](https://pnpm.io/)（v8+）

### 构建步骤

```bash
git clone https://github.com/ZRainbow1275/bentodesk.git
cd bentodesk

# 安装前端依赖
pnpm install

# 开发模式（热重载）
pnpm tauri dev

# 构建生产安装包
pnpm tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`。

</details>

---

## 技术架构

| 层 | 技术 |
|---|------|
| 后端 | **Rust** + **Tauri v2** |
| 前端 | **SolidJS** + **TypeScript** |
| 渲染 | **WebView2**（Chromium） |
| 桌面集成 | **Win32 API**（COM / DWM / Shell / HiDpi） |
| 构建 | **Vite**（前端）+ **Cargo**（后端） |
| 打包 | **NSIS** / **MSI** 安装器 |

<details>
<summary>项目结构</summary>

```
bentodesk/
├── src/                          # 前端（SolidJS + TypeScript）
│   ├── components/               # UI 组件
│   │   ├── BentoZone/            # 便当区域
│   │   ├── Settings/             # 设置面板
│   │   ├── SmartGroup/           # 智能分组
│   │   └── ...
│   ├── i18n/locales/             # 语言文件（zh-CN / en）
│   ├── services/                 # IPC 通信、拖放、事件
│   ├── stores/                   # 响应式状态管理
│   └── themes/                   # 主题系统
├── src-tauri/                    # 后端（Rust + Tauri v2）
│   └── src/
│       ├── commands/             # IPC 命令处理
│       ├── ghost_layer/          # 桌面幽灵覆盖层（Win32）
│       ├── drag_drop/            # 原生 OLE 拖放
│       ├── icon/                 # 图标提取与缓存
│       ├── watcher/              # 文件系统监视
│       └── ...
└── docs/                         # 开发者文档
```

</details>

---

## 参与贡献

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建特性分支 `git checkout -b feature/amazing-feature`
3. 提交更改 `git commit -m 'feat: add amazing feature'`
4. 推送分支 `git push origin feature/amazing-feature`
5. 创建 Pull Request

---

## 开源协议

[AGPL-3.0](LICENSE) — 自由使用与修改，衍生作品须以相同协议开源。

---

<p align="center">
  <sub>用 Rust 和热爱构建 ❤️</sub>
</p>
