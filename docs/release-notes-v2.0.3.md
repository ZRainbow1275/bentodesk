# BentoDesk 2.0.3

## English

BentoDesk 2.0.3 bounds data read from files, plugins, themes, icon caches,
watchers, and update responses before it can grow resident memory. Drag-and-drop
staging now keeps copy and move decisions inside the validated Shell path, and
graphics recovery avoids retaining stale render resources.

The release chain is stricter too. Every accepted `main` commit declares a new
workspace version and is published under the matching immutable annotated tag
only after the source, supply-chain, package, checksum, and extracted-runtime
gates pass. The product remains one native Win32 / Direct2D /
DirectComposition process, with English and Simplified Chinese interfaces.

Download `BentoDesk-2.0.3-windows-x64-portable.zip`, verify it with
`SHA256SUMS.txt`, extract it, and run `BentoDesk.exe`.

## 简体中文

BentoDesk 2.0.3 为文件、插件、主题、图标缓存、watcher 与更新响应补齐读取上限，
避免外部输入无界推高常驻内存。拖放暂存只在经过校验的 Shell 路径内决定复制或
移动；图形恢复也不再保留过期渲染资源。

发布链同时收紧：以后每个通过审核的 `main` 提交都必须声明新版本，并且只有在
源码、供应链、打包、校验和与解包运行闸门全部通过后，才会以匹配的不可变
annotated tag 正式发布。产品仍是单进程 Win32 / Direct2D /
DirectComposition 原生应用；默认界面随系统 UI 语言选择，并支持 English
与简体中文切换。

下载 `BentoDesk-2.0.3-windows-x64-portable.zip`，使用 `SHA256SUMS.txt`
核验后解压并运行 `BentoDesk.exe`。
