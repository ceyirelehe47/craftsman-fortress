# R4 决策记录

## R4-01 · 对象不是体素

角色、家具、工具、设施和环境物件采用独立对象层。素材内部的小体素只是 Mesh 外观，不进入 `BlockId` 或 Chunk 数据。

## R4-02 · 持久身份与 ECS 解耦

`ObjectId(u64)` 是存档身份；ECS Entity 仅是运行期表现。加载、流送或视觉重建都可产生新的 Entity，而对象 ID 不变。

## R4-03 · 稳定类型映射素材 ID

对象存档只保存 `campfire_basic` 等稳定类型 ID。类型目录再映射到 `bonefire` 等 R3 稳定素材 ID；GLB 路径、Scene0、节点名和 Mesh 索引不得进入永久存档。

## R4-04 · 整数空间契约

对象位置采用整数锚点和四向旋转。占地、净空、支撑与地形冲突全部可确定性复算，避免用视觉 AABB 作为玩法权威数据。

## R4-05 · CFSAVE02 保持冻结

对象使用独立 `CFOBJ001` 双槽伴随格式。R4 不修改 R2.1 地形存档，以便后续分别演化地形区域文件和对象区域文件。

## R4-06 · 对象预写、地形后提交

保存时先写对象新槽并保护旧耐久配对槽，再写地形。对象槽通过 terrain semantic hash 与地形配对；未完成的预写不会覆盖可恢复旧状态。

## R4-07 · 第三方素材仍受 R3.1 边界保护

完整 R4 验收从受控私有缓存重新建立 R3.1 来源链，视觉证据只包含截图和元数据，不包含模型、贴图或原包字节。

## R4-08 · 应用参考补丁后的等价修正

制包环境未运行 Rust 编译（任务包 PATCH_VALIDATION 已声明），应用补丁后做了以下不改外部行为的修正：

- `src/objects.rs`：`yaw_quarters % 2 == 0` 改为 `yaw_quarters.is_multiple_of(2)`（本机 clippy 新 lint `manual_is_multiple_of`）。
- `examples/r4_object_lab.rs`：两处 E0502 借用错误改为先取值再借用（manifest 路径先 clone，截图路径先拼接），逻辑与求值顺序不变。
- `scripts/publish_r4_release.sh`：照 R3.1 先例补 `command -v unzip` 预检。

## R4-09 · E02 拒绝分支自动负例

影响范围审查指出参考测试未直接断言越界、顶层 H-1 占用与净空内地形实体三个拒绝分支。`src/objects.rs` 新增 `placement_rejects_out_of_bounds_top_layer_and_terrain` 负例测试，并断言失败放置不推进 `next_id`、不改变存储状态（失败无副作用）。这提高了自动覆盖，不改变任何门槛。
