# BentoDesk 2.0.7

## English

BentoDesk 2.0.7 fixes the two remaining desktop-item defects. Native Windows
icons now keep their real colour and alpha, including legacy mask-based and
monochrome resources. Dragging an item out follows Explorer semantics: a normal
drag requests a move and removes the Zone card only after Windows confirms that
move; Ctrl-drag copies and keeps the card. Cancelled or ambiguous drops never
remove it.

This release also adds a native Windows x64 Setup executable alongside the
portable ZIP. Setup requires acceptance of the User Agreement and Privacy
Policy, records the author and contact details, and installs per user without
administrator privileges. Uninstall preserves BentoDesk settings by default;
an explicit, separately confirmed option removes only BentoDesk state and never
the desktop files or folders represented by Zones.

BentoDesk now has no network client. Updates are downloaded manually from
GitHub Releases; optional local update manifests and packages are verified
entirely offline.

## 简体中文

BentoDesk 2.0.7 修复最后两处桌面项目问题。Windows 原生图标现在能正确保留
颜色与透明度，也覆盖旧式遮罩图标和单色资源。把项目拖出 Zone 时遵循 Explorer
语义：普通拖拽请求移动，只有 Windows 明确确认移动后才移除 Zone 卡片；按住
Ctrl 拖拽则复制并保留卡片。取消或结果不明确的拖放不会移除卡片。

本版在便携 ZIP 之外新增 Windows x64 原生安装程序。安装前必须同意用户协议与
隐私政策；安装程序会显示作者和联系方式，并以当前用户权限安装，不请求管理员
权限。卸载默认保留 BentoDesk 设置；只有另行选择并再次确认后才会清除 BentoDesk
自身状态，绝不会删除 Zone 中展示的桌面文件或文件夹。

BentoDesk 现已不包含网络客户端。新版本由用户从 GitHub Releases 手动下载；
可选的本地更新清单与安装包校验也完全离线完成。
