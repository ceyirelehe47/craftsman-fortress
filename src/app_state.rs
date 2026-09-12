//! 应用状态机。

use bevy::prelude::States;

/// 全局游戏状态。
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    /// 世界生成 + Chunk 网格构建（带进度显示）。
    #[default]
    Loading,
    /// 准备完毕：相机控制、拾取、验收巡航可用。
    Ready,
    /// 验收收尾（写报告/证据），随后退出。
    Finished,
}
