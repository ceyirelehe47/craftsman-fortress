# R2.1 独立复核报告

- **代码验收提交 S**：`__SHA__`
- **实现方证据包 / SHA-256**：`__IMPL__`
- **Reviewer 证据包 / SHA-256**：`__REVIEW__`
- **结论**：`PASS | FAIL`

## 隔离性

- 全新克隆：`__PATH__`
- 干净工作区：`__STATUS__`
- Bevy：`0.19.1`

## R1 回归

A01–A14：`__RESULT__`

## R2.1 B01–B12

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| B01 | | |
| B02 | | |
| B03 | | |
| B04 | | |
| B05 | | |
| B06 | | |
| B07 | | |
| B08 | | |
| B09 | | |
| B10 | | |
| B11 | | |
| B12 | | |

## 必查场景

- base hash 错误且 FNV 已重算：
- world hash 错误且 FNV 已重算：
- 合法记录篡改且 FNV 已重算：
- 两槽语义无效：
- 保存期间唯一完整有效旧槽保持不变：
- generation 溢出拒绝：
- 损坏主程序加载退出码 3：
- headless watchdog：

## Release

- 标签 `r2.1-baseline-<S短SHA>` 解引用到 S：
- 历史 `r2-baseline-3b5ae690e349` 未移动：
