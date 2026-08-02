# BentoDesk 2.0.5

## English

BentoDesk 2.0.5 keeps the native 2.0.4 product behavior and fixes Release
identity preservation. Every create, draft update, and publish request now
repeats the exact tag and source commit, preventing GitHub from converting a
versioned draft into an `untagged-*` draft during normalization.

Draft discovery, asset upload, digest checks, and publication remain bound to
one numeric Release ID. Duplicate drafts and published Releases still fail
closed, and published tags or assets are never moved or overwritten.

Download `BentoDesk-2.0.5-windows-x64-portable.zip`, verify it with
`SHA256SUMS.txt`, extract it, and run `BentoDesk.exe`.

## 简体中文

BentoDesk 2.0.5 保持原生 2.0.4 的产品行为，修复 Release 身份保持问题。创建、
更新草稿与正式发布时都会重复提交精确 tag 和源码 commit，避免 GitHub 在草稿
标准化时把版本化草稿改成 `untagged-*`。

草稿查找、资产上传、摘要校验与发布仍全程绑定同一个 numeric Release ID。
重复草稿和已发布 Release 继续 fail-closed，已经发布的 tag 与资产绝不移动或覆盖。

下载 `BentoDesk-2.0.5-windows-x64-portable.zip`，使用 `SHA256SUMS.txt`
核验后解压并运行 `BentoDesk.exe`。
