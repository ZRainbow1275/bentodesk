# BentoDesk 2.0.8

## English

BentoDesk 2.0.8 supersedes 2.0.7 with one release-engineering safety fix.
The native installer lifecycle verifier now refuses to run before touching the
filesystem or registry unless it is explicitly invoked on a GitHub-hosted
Windows runner. CI state remains isolated under the runner's temporary
directory, while the disposable profile still exercises real install,
upgrade, repair, launch, preserve-uninstall and confirmed-purge behavior.

The application itself is unchanged from 2.0.7: real Windows icons retain
their colour and alpha, successful Explorer moves remove the source Zone card,
and cancelled, copied or ambiguous drops keep it. Setup remains per-user,
offline and bilingual; uninstall preserves settings unless deletion is
separately selected and confirmed.

## 简体中文

BentoDesk 2.0.8 取代 2.0.7，补上一项发布工程安全修复。原生安装生命周期
验证器现在只允许在明确指定的 GitHub 托管 Windows Runner 上运行；不符合条件时，
会在读取安装包、写入文件或注册表之前直接退出。CI 状态仍隔离在 Runner 临时目录，
一次性用户环境继续验证真实安装、升级、修复、启动、保留设置卸载和确认清除流程。

应用功能与 2.0.7 保持一致：Windows 图标保留正确颜色与透明度；Explorer 确认移动
后才移除 Zone 卡片；复制、取消或结果不明确的拖放仍保留卡片。安装程序继续采用
当前用户权限、完全离线并提供中英双语；卸载默认保留设置，只有另行选择并确认后
才会清除 BentoDesk 自身状态。
