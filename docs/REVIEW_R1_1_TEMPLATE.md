# R1.1 独立复核报告

- **代码验收提交 S**：`__BASELINE_SHA__`
- **实现方 run_id**：`__IMPL_RUN_ID__`
- **Reviewer run_id**：`__REVIEW_RUN_ID__`
- **实现方命令**：`__IMPL_COMMAND__`
- **Reviewer 命令**：`__REVIEW_COMMAND__`
- **最终结论**：`__OVERALL__`

## 1. 提交与工作区

- 实现方从全新克隆检出 `S`，工作区：`__IMPL_WORKTREE_STATUS__`
- Reviewer 从独立全新克隆检出 `S`，工作区：`__REVIEW_WORKTREE_STATUS__`
- `S` 的 CI：`__BASELINE_CI_URL__`

## 2. R1.1 回归修复

| 检查 | 结论 | 证据 |
|---|---|---|
| 40.5m 高速位移不能穿一格厚墙 | __SWEEP_PASS__ | `high_speed_interactive_move_cannot_tunnel_through_thin_wall` |
| 高速斜向移动可沿墙滑动 | __SLIDE_PASS__ | `high_speed_diagonal_move_slides_along_thin_wall` |
| H-2 实体列恢复到 H-1 空气层 | __TOP_AIR_PASS__ | `focus_recovery_can_use_top_air_layer` |

## 3. A01–A14

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| A01 | __A01__ | __A01_EVIDENCE__ |
| A02 | __A02__ | __A02_EVIDENCE__ |
| A03 | __A03__ | __A03_EVIDENCE__ |
| A04 | __A04__ | __A04_EVIDENCE__ |
| A05 | __A05__ | __A05_EVIDENCE__ |
| A06 | __A06__ | __A06_EVIDENCE__ |
| A07 | __A07__ | __A07_EVIDENCE__ |
| A08 | __A08__ | __A08_EVIDENCE__ |
| A09 | __A09__ | __A09_EVIDENCE__ |
| A10 | __A10__ | __A10_EVIDENCE__ |
| A11 | __A11__ | __A11_EVIDENCE__ |
| A12 | __A12__ | __A12_EVIDENCE__ |
| A13 | __A13__ | __A13_EVIDENCE__ |
| A14 | __A14__ | 本报告 |

## 4. 独立复算

### A03

__A03_RECALC__

### A07

__A07_RECALC__

### A10

__A10_RECALC__

### A12

__A12_RECALC__

### A13

__A13_RECALC__

## 5. 截图与 GIF 目检

__VISUAL_REVIEW__

## 6. 证据包

| 文件 | SHA-256 |
|---|---|
| `evidence_impl___BASELINE_SHORT__.zip` | `__IMPL_ZIP_SHA256__` |
| `evidence_review___BASELINE_SHORT__.zip` | `__REVIEW_ZIP_SHA256__` |

两个 ZIP 内的 `commit.txt` 均为 `__BASELINE_SHA__`，且内部 `report.json` 均为 PASS。

## 7. Release 预期

- 计划标签：`r1.1-baseline-__BASELINE_SHORT__`
- 标签必须解析到：`__BASELINE_SHA__`
- 实际 Release URL 与发布复核结果在最终交付清单中记录；本报告不写入自身提交 SHA 或发布后才产生的 CI/URL，避免自引用。

## 8. 非阻断观察项

__NON_BLOCKING_OBSERVATIONS__
