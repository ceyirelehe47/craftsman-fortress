//! 《工匠要塞：机械纪元（暂名）》初版验证构建。
//!
//! 任务书 V0.1 切片：底层框架 + 世界生成 + 自由透视相机。
//! 模块职责边界见 `docs/ARCHITECTURE.md`。

pub mod acceptance;
pub mod app;
pub mod app_state;
pub mod camera;
pub mod chunk;
pub mod config;
pub mod coords;
pub mod diagnostics;
pub mod features;
pub mod generation;
pub mod meshing;
pub mod noise;
pub mod picking;
pub mod render;
pub mod voxel;
pub mod world;
