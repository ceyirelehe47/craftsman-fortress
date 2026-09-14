# R3.1 独立复核报告

- 代码验收提交 S：`__SHA__`
- 基线：`r3-baseline-355344c10846`
- 实现方证据 / SHA-256：`__IMPL__`
- Reviewer 证据 / SHA-256：`__REVIEW__`
- 结论：`PASS | FAIL`

## 隔离性与自主取得

- Reviewer 全新克隆：
- 工作区干净：
- Bevy：`0.19.1`
- 素材来源：`private_cache | private_url`
- 用户交互：必须 `none`
- 原包 SHA-256 / 字节数：
- 实现方与 Reviewer 是否从各自克隆运行 acquire + prepare：

## 来源链

- actual source_receipt SHA 等于 selection / lock：
- inventory SHA 等于 lock：
- selection SHA 等于 lock：
- inventory 每个 GLB/PNG 重新计算通过：
- selection.lock 每项与 selection + inventory + 实际文件一致：
- asset_set_sha256（实现方 / Reviewer）：

## 回归

- A01–A14：
- B01–B12：
- C01–C12：
- CFSAVE02 未变化：

## R3.1 D01–D12

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| D01 | | |
| D02 | | |
| D03 | | |
| D04 | | |
| D05 | | |
| D06 | | |
| D07 | | |
| D08 | | |
| D09 | | |
| D10 | | |
| D11 | | |
| D12 | | |

## 必查实验

- 修改选中 GLB 1 字节后来源链失败：
- 修改 source_receipt 后来源链失败：
- 把第三方 PNG 改名放入 ZIP 后泄漏守卫失败：
- 嵌套 ZIP 泄漏失败：
- 两次 lab 日志 B0004 命中 0：
- cycle_1..cycle_5 每轮都回到基线：
- 实现方与 Reviewer manifest / transform / asset-set 三指纹一致：

## Release

- 标签 `r3.1-baseline-<S短SHA>` 解引用到 S：
- 历史 `r3-baseline-355344c10846` 未移动：
- Release 仅含实现方证据、Reviewer 证据与 SHA256SUMS：
- `verify_r3_1_release.sh` 从空目录下载复验通过：
