# BentoDesk 2.2.0

## English

BentoDesk 2.2.0 completes expanded Zone resizing with the four full edges and
four corners. Edge drags resize one axis, corner drags resize both axes, Windows
resize cursors match the active handle, and the opposite edge stays fixed while
the saved Zone geometry remains stable across restart.

Item cards now adapt their icon and label layout to the available card size.
Higher-resolution native icon sources refresh the old cache automatically;
wrapped labels, stable row geometry, shared scrolling, and centered hover
enlargement keep paint, hit testing, search, Stack previews, and drag-and-drop
aligned.

The Windows updater now uses the default official endpoint
`https://api.github.com/repos/ZRainbow1275/bentodesk/releases/latest` over
HTTPS. A controlled `BENTODESK_UPDATE_MANIFEST_URL` may select an HTTPS, file, or
local override. Scheduled checks run immediately and then at the selected cadence
(Weekly by default; Manual disables scheduled checks). Auto-download is enabled
by default. On the default channel, it stages the exact official setup only
after its GitHub SHA-256 digest is verified. A controlled override stages its
declared artifact only after manifest SHA-256 or embedded-key minisign
verification. Installation always requires an explicit user action.

BentoDesk remains local-first: there is no account, project-operated server,
telemetry, analytics, advertising, cloud storage, crash upload, or upload of
desktop file paths, names, icons, or contents. GitHub, or the host selected by a
controlled HTTPS override, can receive ordinary HTTPS request metadata such as
the source IP address and BentoDesk User-Agent under its own policies.

## 简体中文

BentoDesk 2.2.0 补齐了展开 Zone 的四条整边与四个角缩放。拖动边缘时只改变
单轴，拖动角落时同时改变双轴；Windows 缩放光标与当前把手一致，对边保持固定，
保存后的 Zone 几何在重启后继续稳定。

ItemCard 的图标与名称布局现在会随真实可用尺寸调整。更高分辨率的原生图标源会
自动刷新旧缓存；完整换行、稳定行高、共享滚动与居中的悬停放大让绘制、命中、
搜索、Stack 预览及拖放保持一致。

Windows 更新器现在默认通过 HTTPS 使用 BentoDesk 官方 endpoint：
`https://api.github.com/repos/ZRainbow1275/bentodesk/releases/latest`。受控的
`BENTODESK_UPDATE_MANIFEST_URL` 也可选择 HTTPS、file 或本地覆盖源；启动后立即执行
一次，之后按所选周期运行（默认每周；Manual 只禁用定时检查）。
默认开启自动下载。默认通道只会在核对 GitHub SHA-256 digest 后暂存精确的官方
安装包；受控覆盖源声明的安装包也必须先通过 manifest SHA-256 或内置公钥 minisign
签名校验。安装始终需要用户明确操作。

BentoDesk 仍以本地运行为主：没有账号、项目自建服务器、遥测、分析、广告、云存储
或崩溃上传，也不会上传桌面文件路径、名称、图标或内容。GitHub 或受控 HTTPS
覆盖源的主机可能按其自身政策接收源 IP 地址、BentoDesk User-Agent 等普通 HTTPS
请求元数据。
