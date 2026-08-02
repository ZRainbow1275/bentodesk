# BentoDesk 2.0.6

## English

BentoDesk 2.0.6 keeps the native product behavior of 2.0.5 and hardens the
portable release path.

Both checksum manifests now use UTF-8 without a BOM and LF line endings. The
Release workflow verifies them with GNU `sha256sum -c`, so the published files
work directly in PowerShell, Git Bash, WSL, and Linux.

Release builds may still run in parallel, but publication now passes through a
single queued writer. That writer compares every published stable semantic
version before and after publication, so an older build that finishes later
cannot replace a newer version as latest. Numeric Release ID, tag, source,
asset-digest, and immutable-publication checks remain unchanged. Version
ordering rejects noncanonical leading zeros and is not limited to 32-bit
components. Release identities are case-exact, and whole-value validators
reject trailing line breaks.

Download `BentoDesk-2.0.6-windows-x64-portable.zip`, verify it with
`SHA256SUMS.txt`, extract it, and run `BentoDesk.exe`.

## 简体中文

BentoDesk 2.0.6 保持 2.0.5 的原生产品行为，继续收紧便携包发布链。

包内外两份校验清单现统一使用无 BOM 的 UTF-8 与 LF 换行，并在 Release
工作流中通过 GNU `sha256sum -c` 实测，因此可直接用于 PowerShell、Git Bash、
WSL 与 Linux。

Release 构建仍可并行，但发布写入统一通过单一队列串行执行。发布器会在写入
前后比较全部已公开稳定版本的语义版本，因此较旧构建即使更晚完成，也不会
重新覆盖较新版本的 latest 身份。numeric Release ID、tag、源码、资产摘要与
不可变发布校验保持不变；版本排序会拒绝带前导零的非规范 tag，也不受 32 位
版本组件上限影响。
版本身份同时按大小写精确核验，整值校验也会拒绝末尾换行。

下载 `BentoDesk-2.0.6-windows-x64-portable.zip`，使用 `SHA256SUMS.txt`
核验后解压并运行 `BentoDesk.exe`。
