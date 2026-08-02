# BentoDesk 2.0.4

## English

BentoDesk 2.0.4 is a release-pipeline maintenance update. It keeps the native
2.0.3 product behavior while correcting draft publication: the workflow now
discovers draft Releases through the paginated Releases API, binds every later
check to the numeric Release ID, and reads by tag only after publication.

Duplicate drafts and already-published Releases are rejected. Assets are still
uploaded only after the read-only build job passes source, supply-chain,
package, checksum, and extracted-runtime checks; publication remains immutable.

Download `BentoDesk-2.0.4-windows-x64-portable.zip`, verify it with
`SHA256SUMS.txt`, extract it, and run `BentoDesk.exe`.

## 简体中文

BentoDesk 2.0.4 是一次发布链维护更新。产品行为与原生 2.0.3 保持一致，修复了
GitHub draft 发布流程：工作流改为通过分页 Releases API 查找草稿，后续校验和
发布全程绑定 numeric Release ID，仅在正式发布后才按 tag 回读。

重复 draft 与已经发布的 Release 会被直接拒绝。资产仍只会在只读构建任务通过
源码、供应链、打包、校验和与解包运行闸门后上传，并继续以不可变 Release 发布。

下载 `BentoDesk-2.0.4-windows-x64-portable.zip`，使用 `SHA256SUMS.txt`
核验后解压并运行 `BentoDesk.exe`。
