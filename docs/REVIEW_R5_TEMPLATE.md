# R5 独立 Reviewer 复核报告

- 代码验收提交 S：`__SHA__`
- 基线：`r4-baseline-1c05cfafce2e`
- 实现方证据 / SHA-256：
- Reviewer 证据 / SHA-256：
- 结论：`PASS | FAIL`

## 隔离性

- 全新克隆：
- 工作区干净：
- Bevy：`0.19.1`
- 私有素材缓存自主取得：
- 用户交互：`none`
- `src/persistence.rs`、`CFSAVE02`、`CFOBJ001` 兼容性：

## 回归

- A01–A14：
- B01–B12：
- C01–C12：
- D01–D12：
- E01–E12：

## R5 F01–F12

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| F01 | | |
| F02 | | |
| F03 | | |
| F04 | | |
| F05 | | |
| F06 | | |
| F07 | | |
| F08 | | |
| F09 | | |
| F10 | | |
| F11 | | |
| F12 | | |

## 必查实验

- 八种稳定构件类型均可出现，稳定 ID 与 ECS Entity 无关：
- 同一墙边正反方向描述归一化并互斥：
- 门洞/窗与墙共享槽，楼梯低高端必须连接楼板：
- 抬高楼板四角支撑，删除任一必要支撑失败且无副作用：
- 失败移动保持原记录、ID、next_id、revision 与语义哈希：
- 地形编辑、独立对象和建筑互相阻挡正确：
- 相同构件集合不同输入顺序，语义哈希一致：
- `CFBLD001` 字节级严格解码与资源上限：
- terrain/object/building 精确往返：
- 最新建筑槽损坏后回退：
- 建筑预写、对象预写后地形未提交，旧三层仍恢复；提交后新三层生效：
- R4 terrain/object 存档无建筑槽时加载为空建筑层：
- 两次 visual lab 的 building hash 和四张截图 SHA 一致：
- 最终证据无递归 ZIP 套娃、无第三方模型/贴图/原包字节：

## Release

- 标签 `r5-baseline-<S短SHA>` 解引用到 S：
- 历史标签 `r4-baseline-1c05cfafce2e` 未移动：
- Release 只含实现方证据、Reviewer 证据和校验清单：
- 发布后下载复验：
