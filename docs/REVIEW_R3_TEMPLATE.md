# R3 独立复核报告

- 代码验收提交 S：`__SHA__`
- 基线：`r2.1-baseline-804df0d8513a`
- 实现方证据 / SHA-256：`__IMPL__`
- Reviewer 证据 / SHA-256：`__REVIEW__`
- 结论：`PASS | FAIL`

## 隔离性与素材来源

- 全新克隆：
- 干净工作区：
- Bevy：`0.19.1`
- 原包名称与 SHA-256：
- 来源页面与取得日期：
- 授权收据 SHA-256：
- Git/Release 中第三方模型文件数量（必须 0）：

## R1 / R2.1 回归

- A01–A14：
- B01–B12：
- CFSAVE02 magic / format / generator revision 未变化：

## R3 C01–C12

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| C01 | | |
| C02 | | |
| C03 | | |
| C04 | | |
| C05 | | |
| C06 | | |
| C07 | | |
| C08 | | |
| C09 | | |
| C10 | | |
| C11 | | |
| C12 | | |

## 必查项目

- 所有清单 GLB 均成功加载：
- 原始/最终包围盒有限且高度正确：
- 最低 Y 落地误差 ≤0.02 m：
- 五张固定截图逐张目检：
- 两次运行 manifest hash 一致：
- 两次运行 Transform 指纹一致：
- 生命周期资源计数回到基线：
- 证据 ZIP 不含 `.glb/.gltf/.fbx/.obj/.rar`：

## Release

- 标签 `r3-baseline-<S短SHA>` 解引用到 S：
- Release 只含两套证据 ZIP 与校验清单，不含第三方模型：
