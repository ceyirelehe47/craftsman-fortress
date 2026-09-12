//! 验收世界预设参数扫描工具（一次性诊断用途，非游戏运行入口）。
//!
//! 用法：cargo run --release --example feature_scan -- [seed ...]
//! 对每个 seed 用当前 `TerrainParams::new` 生成 256×128×256 世界并输出特征探测摘要。
//! 用于为"固定验收世界预设"选择参数/seed（任务书 4.3：不依赖人工反复换 Seed）。

use craftsman_fortress::coords::WorldSize;
use craftsman_fortress::features::probe_world;
use craftsman_fortress::generation::TerrainParams;
use craftsman_fortress::world::World;

fn main() {
    let args: Vec<u64> = std::env::args()
        .skip(1)
        .map(|s| s.parse().expect("参数必须为 u64 seed"))
        .collect();
    let seeds: Vec<u64> = if args.is_empty() {
        vec![20260912]
    } else {
        args
    };
    for seed in seeds {
        let t0 = std::time::Instant::now();
        let world = World::generate_all(WorldSize::new(256, 128, 256), TerrainParams::new(seed));
        let gen_s = t0.elapsed().as_secs_f32();
        let report = probe_world(&world);
        println!(
            "seed={seed} all_present={} gen={gen_s:.1}s :: {}",
            report.all_present(),
            report.summary()
        );
    }
}
