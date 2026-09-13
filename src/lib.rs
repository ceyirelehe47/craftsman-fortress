//! 工匠要塞 初版验证构建。
//!
//! 底层框架 + 确定性世界生成 + 自由透视相机。
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
pub mod r1_checks;
pub mod render;
pub mod voxel;
pub mod world;
