//! 启动配置（Seed、世界尺寸、存档路径与必要调试开关）。
//!
//! 参数形式：`--key value` 手写解析，不引入 clap。
//! 交互运行：`craftsman-fortress [--seed N] [--size X Y Z] [--save PATH] [--load PATH]`
//! 验收运行：`craftsman-fortress --acceptance [--evidence DIR]`

use crate::coords::WorldSize;

pub const DEFAULT_SEED: u64 = 12345;
pub const DEFAULT_SIZE: WorldSize = WorldSize::new(256, 128, 256);
pub const ACCEPTANCE_WINDOW: [f32; 2] = [1280.0, 720.0];
pub const DEFAULT_SAVE_PATH: &str = "saves/quicksave.cfsv";

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub seed: u64,
    pub size: WorldSize,
    pub window: [f32; 2],
    pub acceptance: bool,
    pub evidence_dir: String,
    pub tint_debug: bool,
    /// 双槽存档的逻辑路径；实际文件为 `<path>.slot0/.slot1`。
    pub save_path: String,
    /// 启动时加载的逻辑路径。加载成功后 seed/size 以存档为准。
    pub load_path: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            seed: DEFAULT_SEED,
            size: DEFAULT_SIZE,
            window: [1280.0, 720.0],
            acceptance: false,
            evidence_dir: "evidence".into(),
            tint_debug: false,
            save_path: DEFAULT_SAVE_PATH.into(),
            load_path: None,
        }
    }
}

impl AppConfig {
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Self, String> {
        let mut cfg = AppConfig::default();
        let args: Vec<String> = args.into_iter().collect();
        let mut i = 0;
        let mut save_explicit = false;
        fn next_at(args: &[String], i: &mut usize, name: &str) -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or_else(|| format!("参数 {name} 缺少取值"))
        }
        while i < args.len() {
            match args[i].as_str() {
                "--seed" => {
                    let v = next_at(&args, &mut i, "--seed")?;
                    cfg.seed = v.parse().map_err(|_| format!("无效 seed: {v}"))?;
                }
                "--size" => {
                    let x: u32 = next_at(&args, &mut i, "--size")?
                        .parse()
                        .map_err(|_| "无效尺寸 X".to_string())?;
                    let y: u32 = next_at(&args, &mut i, "--size")?
                        .parse()
                        .map_err(|_| "无效尺寸 Y".to_string())?;
                    let z: u32 = next_at(&args, &mut i, "--size")?
                        .parse()
                        .map_err(|_| "无效尺寸 Z".to_string())?;
                    validate_size(x, y, z)?;
                    cfg.size = WorldSize::new(x, y, z);
                }
                "--window" => {
                    let w: f32 = next_at(&args, &mut i, "--window")?
                        .parse()
                        .map_err(|_| "无效窗口宽度".to_string())?;
                    let h: f32 = next_at(&args, &mut i, "--window")?
                        .parse()
                        .map_err(|_| "无效窗口高度".to_string())?;
                    if w < 320.0 || h < 240.0 {
                        return Err("窗口尺寸过小".into());
                    }
                    cfg.window = [w, h];
                }
                "--acceptance" => cfg.acceptance = true,
                "--evidence" => cfg.evidence_dir = next_at(&args, &mut i, "--evidence")?,
                "--tint" => cfg.tint_debug = true,
                "--save" => {
                    cfg.save_path = next_at(&args, &mut i, "--save")?;
                    save_explicit = true;
                }
                "--load" => cfg.load_path = Some(next_at(&args, &mut i, "--load")?),
                "--help" | "-h" => {
                    print_usage();
                    return Err("--help 退出".into());
                }
                other => return Err(format!("未知参数: {other}")),
            }
            i += 1;
        }
        if cfg.acceptance && cfg.load_path.is_some() {
            return Err("--acceptance 不允许加载用户存档".into());
        }
        if let Some(load) = cfg.load_path.as_ref() {
            if !save_explicit {
                cfg.save_path = load.clone();
            }
        }
        Ok(cfg)
    }
}

fn validate_size(x: u32, y: u32, z: u32) -> Result<(), String> {
    const CHUNK: u32 = 16;
    if x == 0 || y == 0 || z == 0 {
        return Err("尺寸必须为正".into());
    }
    if !x.is_multiple_of(CHUNK) || !y.is_multiple_of(CHUNK) || !z.is_multiple_of(CHUNK) {
        return Err("尺寸必须是 16 的整数倍（Chunk 尺寸）".into());
    }
    if x < 256 || z < 256 || y < 128 {
        return Err("尺寸低于验证下限 256×128×256".into());
    }
    if x > 512 || y > 256 || z > 512 {
        return Err("尺寸超出初版验证上限 512×256×512".into());
    }
    Ok(())
}

fn print_usage() {
    println!(
        "工匠要塞（初版验证构建）\n\
         用法: craftsman-fortress [选项]\n\
         \x20 --seed N          世界种子（默认 {DEFAULT_SEED}）\n\
         \x20 --size X Y Z      世界尺寸（体素，默认 256 128 256）\n\
         \x20 --window W H      窗口分辨率（默认 1280 720）\n\
         \x20 --save PATH       手动 F5 保存的逻辑路径（默认 {DEFAULT_SAVE_PATH}）\n\
         \x20 --load PATH       从双槽存档加载；未给 --save 时原路径回写\n\
         \x20 --acceptance      验收模式（脚本巡航 + 证据 + 自动判定）\n\
         \x20 --evidence DIR    证据输出目录（默认 evidence）\n\
         \x20 --tint            启动即开启 Chunk 边界调试着色\n\
         \x20 --help            显示本帮助"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_defaults() {
        let cfg = AppConfig::parse(Vec::new()).unwrap();
        assert_eq!(cfg.seed, DEFAULT_SEED);
        assert_eq!(cfg.size, WorldSize::new(256, 128, 256));
        assert_eq!(cfg.save_path, DEFAULT_SAVE_PATH);
        assert!(cfg.load_path.is_none());
        assert!(!cfg.acceptance);
    }

    #[test]
    fn parse_acceptance() {
        let cfg =
            AppConfig::parse(args(&["--acceptance", "--seed", "42", "--evidence", "out"])).unwrap();
        assert!(cfg.acceptance);
        assert_eq!(cfg.seed, 42);
        assert_eq!(cfg.evidence_dir, "out");
    }

    #[test]
    fn load_becomes_default_save_target() {
        let cfg = AppConfig::parse(args(&["--load", "saves/a.cfsv"])).unwrap();
        assert_eq!(cfg.load_path.as_deref(), Some("saves/a.cfsv"));
        assert_eq!(cfg.save_path, "saves/a.cfsv");
        let cfg =
            AppConfig::parse(args(&["--load", "saves/a.cfsv", "--save", "saves/b.cfsv"])).unwrap();
        assert_eq!(cfg.save_path, "saves/b.cfsv");
    }

    #[test]
    fn parse_rejects_bad() {
        assert!(AppConfig::parse(args(&["--seed", "abc"])).is_err());
        assert!(AppConfig::parse(args(&["--size", "100", "128", "256"])).is_err());
        assert!(AppConfig::parse(args(&["--size", "256", "64", "256"])).is_err());
        assert!(AppConfig::parse(args(&["--nope"])).is_err());
        assert!(AppConfig::parse(args(&["--seed"])).is_err());
        assert!(AppConfig::parse(args(&["--acceptance", "--load", "x"])).is_err());
    }
}
