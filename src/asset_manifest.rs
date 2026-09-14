//! R3 本地第三方素材清单与尺度规范化数学。
//!
//! 真实 PalmStudio 模型不进入公开仓库；代码只读取本地忽略目录中的 GLB。
//! 清单采用可审计的 TSV，并携带来源与授权收据哈希。

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const PALMSTUDIO_SOURCE_URL: &str = "https://palmstudio.itch.io/voxel-survival-pack";
pub const MANIFEST_HEADER: &str = "id\tpath\tcategory\ttarget_height_m\tyaw_deg";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetCategory {
    Character,
    Animal,
    Prop,
    Environment,
    Building,
    Tool,
    Unknown,
}

impl AssetCategory {
    pub fn parse(value: &str) -> Result<Self, ManifestError> {
        match value {
            "character" => Ok(Self::Character),
            "animal" => Ok(Self::Animal),
            "prop" => Ok(Self::Prop),
            "environment" => Ok(Self::Environment),
            "building" => Ok(Self::Building),
            "tool" => Ok(Self::Tool),
            "unknown" => Ok(Self::Unknown),
            _ => Err(ManifestError(format!("unknown category: {value}"))),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Animal => "animal",
            Self::Prop => "prop",
            Self::Environment => "environment",
            Self::Building => "building",
            Self::Tool => "tool",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssetSpec {
    pub id: String,
    /// 相对 Bevy `assets/` 根目录的路径。
    pub path: String,
    pub category: AssetCategory,
    pub target_height_m: f32,
    pub yaw_deg: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssetManifest {
    pub source_url: String,
    pub pack_version: String,
    pub sample_file: String,
    pub archive_sha256: String,
    pub license_receipt_sha256: String,
    pub acquired_at: String,
    pub entries: Vec<AssetSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError(pub String);

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ManifestError {}

impl AssetManifest {
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let text = fs::read_to_string(path)
            .map_err(|e| ManifestError(format!("cannot read {}: {e}", path.display())))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let mut source_url = None;
        let mut pack_version = None;
        let mut sample_file = None;
        let mut archive_sha256 = None;
        let mut license_receipt_sha256 = None;
        let mut acquired_at = None;
        let mut saw_header = false;
        let mut entries = Vec::new();

        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = raw.trim_end_matches('\r').trim();
            if line.is_empty() {
                continue;
            }
            if let Some(meta) = line.strip_prefix('#') {
                let Some((key, value)) = meta.trim().split_once('=') else {
                    return Err(ManifestError(format!(
                        "line {line_no}: metadata must be '# key=value'"
                    )));
                };
                let value = value.trim().to_string();
                match key.trim() {
                    "source_url" => source_url = Some(value),
                    "pack_version" => pack_version = Some(value),
                    "sample_file" => sample_file = Some(value),
                    "archive_sha256" => archive_sha256 = Some(value),
                    "license_receipt_sha256" => license_receipt_sha256 = Some(value),
                    "acquired_at" => acquired_at = Some(value),
                    other => {
                        return Err(ManifestError(format!(
                            "line {line_no}: unknown metadata key {other}"
                        )))
                    }
                }
                continue;
            }
            if !saw_header {
                if line != MANIFEST_HEADER {
                    return Err(ManifestError(format!(
                        "line {line_no}: expected header '{MANIFEST_HEADER}'"
                    )));
                }
                saw_header = true;
                continue;
            }
            let fields: Vec<_> = raw.trim_end_matches('\r').split('\t').collect();
            if fields.len() != 5 {
                return Err(ManifestError(format!(
                    "line {line_no}: expected 5 tab-separated fields, got {}",
                    fields.len()
                )));
            }
            let id = fields[0].trim().to_string();
            let path = fields[1].trim().replace('\\', "/");
            let category = AssetCategory::parse(fields[2].trim())?;
            let target_height_m: f32 = fields[3]
                .trim()
                .parse()
                .map_err(|_| ManifestError(format!("line {line_no}: bad target height")))?;
            let yaw_deg: f32 = fields[4]
                .trim()
                .parse()
                .map_err(|_| ManifestError(format!("line {line_no}: bad yaw")))?;
            validate_id(&id, line_no)?;
            validate_asset_path(&path, line_no)?;
            if !target_height_m.is_finite() || !(0.05..=50.0).contains(&target_height_m) {
                return Err(ManifestError(format!(
                    "line {line_no}: target height must be finite and within 0.05..=50m"
                )));
            }
            if !yaw_deg.is_finite() || yaw_deg.abs() > 3600.0 {
                return Err(ManifestError(format!(
                    "line {line_no}: yaw must be finite and within ±3600 degrees"
                )));
            }
            entries.push(AssetSpec {
                id,
                path,
                category,
                target_height_m,
                yaw_deg,
            });
        }

        if !saw_header {
            return Err(ManifestError("manifest header is missing".into()));
        }
        let manifest = Self {
            source_url: required(source_url, "source_url")?,
            pack_version: required(pack_version, "pack_version")?,
            sample_file: required(sample_file, "sample_file")?,
            archive_sha256: required(archive_sha256, "archive_sha256")?,
            license_receipt_sha256: required(license_receipt_sha256, "license_receipt_sha256")?,
            acquired_at: required(acquired_at, "acquired_at")?,
            entries,
        };
        manifest.validate_structure()?;
        Ok(manifest)
    }

    pub fn validate_structure(&self) -> Result<(), ManifestError> {
        if self.source_url != PALMSTUDIO_SOURCE_URL {
            return Err(ManifestError(format!(
                "source_url must be {PALMSTUDIO_SOURCE_URL}"
            )));
        }
        if self.pack_version.trim().is_empty()
            || self.sample_file.trim().is_empty()
            || self.acquired_at.trim().is_empty()
        {
            return Err(ManifestError(
                "source metadata contains an empty value".into(),
            ));
        }
        validate_sha256(&self.archive_sha256, "archive_sha256")?;
        validate_sha256(&self.license_receipt_sha256, "license_receipt_sha256")?;
        if self.entries.is_empty() {
            return Err(ManifestError("manifest has no assets".into()));
        }
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        for entry in &self.entries {
            if !ids.insert(entry.id.as_str()) {
                return Err(ManifestError(format!("duplicate id: {}", entry.id)));
            }
            if !paths.insert(entry.path.as_str()) {
                return Err(ManifestError(format!("duplicate path: {}", entry.path)));
            }
        }
        Ok(())
    }

    /// R3 正式验收要求：至少三个资产、至少两个类别，且不能保留 unknown。
    pub fn validate_acceptance_ready(&self) -> Result<(), ManifestError> {
        self.validate_structure()?;
        if self.entries.len() < 3 {
            return Err(ManifestError(
                "R3 acceptance requires at least 3 selected GLB assets".into(),
            ));
        }
        if self
            .entries
            .iter()
            .any(|entry| entry.category == AssetCategory::Unknown)
        {
            return Err(ManifestError(
                "R3 acceptance manifest cannot contain category=unknown".into(),
            ));
        }
        let categories: HashSet<_> = self.entries.iter().map(|entry| entry.category).collect();
        if categories.len() < 2 {
            return Err(ManifestError(
                "R3 acceptance requires at least two distinct asset categories".into(),
            ));
        }
        Ok(())
    }

    pub fn validate_files(&self, asset_root: &Path) -> Result<(), ManifestError> {
        for entry in &self.entries {
            let path = asset_root.join(Path::new(&entry.path));
            if !path.is_file() {
                return Err(ManifestError(format!(
                    "asset file is missing: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut feed = |value: &str| {
            for byte in value.as_bytes() {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash ^= b'\n' as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        feed(&self.source_url);
        feed(&self.pack_version);
        feed(&self.sample_file);
        feed(&self.archive_sha256);
        feed(&self.license_receipt_sha256);
        feed(&self.acquired_at);
        for entry in &self.entries {
            feed(&entry.id);
            feed(&entry.path);
            feed(entry.category.as_str());
            feed(&format!("{:.6}", entry.target_height_m));
            feed(&format!("{:.6}", entry.yaw_deg));
        }
        hash
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizationPlan {
    pub raw_min: [f32; 3],
    pub raw_max: [f32; 3],
    pub scale: f32,
    pub ground_offset_y: f32,
    pub target_height_m: f32,
}

impl NormalizationPlan {
    pub fn from_bounds(
        raw_min: [f32; 3],
        raw_max: [f32; 3],
        target_height_m: f32,
    ) -> Result<Self, ManifestError> {
        for value in raw_min.into_iter().chain(raw_max) {
            if !value.is_finite() {
                return Err(ManifestError(
                    "asset bounds contain a non-finite value".into(),
                ));
            }
        }
        let raw_height = raw_max[1] - raw_min[1];
        if raw_height <= 1e-5 {
            return Err(ManifestError(format!(
                "asset raw height is degenerate: {raw_height}"
            )));
        }
        if !target_height_m.is_finite() || target_height_m <= 0.0 {
            return Err(ManifestError(
                "target height must be positive and finite".into(),
            ));
        }
        let scale = target_height_m / raw_height;
        Ok(Self {
            raw_min,
            raw_max,
            scale,
            ground_offset_y: -raw_min[1] * scale,
            target_height_m,
        })
    }

    pub fn predicted_height(self) -> f32 {
        (self.raw_max[1] - self.raw_min[1]) * self.scale
    }
}

fn required(value: Option<String>, key: &str) -> Result<String, ManifestError> {
    value.ok_or_else(|| ManifestError(format!("missing metadata: {key}")))
}

fn validate_id(id: &str, line_no: usize) -> Result<(), ManifestError> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        return Err(ManifestError(format!(
            "line {line_no}: id must match [a-z0-9_-]+"
        )));
    }
    Ok(())
}

fn validate_asset_path(path: &str, line_no: usize) -> Result<(), ManifestError> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute()
        || candidate.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ManifestError(format!(
            "line {line_no}: path must be a safe relative path"
        )));
    }
    if !path.starts_with("vendor_local/voxel_survival_pack/") {
        return Err(ManifestError(format!(
            "line {line_no}: path must stay under vendor_local/voxel_survival_pack/"
        )));
    }
    if !path.to_ascii_lowercase().ends_with(".glb") {
        return Err(ManifestError(format!(
            "line {line_no}: R3 only accepts .glb assets"
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str, key: &str) -> Result<(), ManifestError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ManifestError(format!(
            "{key} must be a 64-character lowercase SHA-256"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_manifest() -> String {
        format!(
            "# source_url={PALMSTUDIO_SOURCE_URL}\n\
             # pack_version=v1.0\n\
             # sample_file=Free_Sample.rar\n\
             # archive_sha256={}\n\
             # license_receipt_sha256={}\n\
             # acquired_at=2026-09-14T00:00:00Z\n\
             {MANIFEST_HEADER}\n\
             worker\tvendor_local/voxel_survival_pack/v1.0/free_sample/worker.glb\tcharacter\t1.80\t180\n\
             crate\tvendor_local/voxel_survival_pack/v1.0/free_sample/crate.glb\tprop\t1.00\t0\n\
             tree\tvendor_local/voxel_survival_pack/v1.0/free_sample/tree.glb\tenvironment\t4.00\t0\n",
            "a".repeat(64),
            "b".repeat(64)
        )
    }

    #[test]
    fn parses_and_hashes_deterministically() {
        let a = AssetManifest::parse(&good_manifest()).unwrap();
        let b = AssetManifest::parse(&good_manifest()).unwrap();
        a.validate_acceptance_ready().unwrap();
        assert_eq!(a.canonical_hash(), b.canonical_hash());
        assert_eq!(a.entries.len(), 3);
    }

    #[test]
    fn rejects_path_traversal_and_unknown_acceptance_category() {
        let bad_path = good_manifest().replace(
            "vendor_local/voxel_survival_pack/v1.0/free_sample/worker.glb",
            "vendor_local/voxel_survival_pack/../worker.glb",
        );
        assert!(AssetManifest::parse(&bad_path).is_err());

        let unknown = good_manifest().replace("\tcharacter\t", "\tunknown\t");
        let parsed = AssetManifest::parse(&unknown).unwrap();
        assert!(parsed.validate_acceptance_ready().is_err());
    }

    #[test]
    fn normalization_matches_target_height_and_ground() {
        let plan =
            NormalizationPlan::from_bounds([-1.0, -0.5, -2.0], [1.0, 1.5, 2.0], 1.8).unwrap();
        assert!((plan.scale - 0.9).abs() < 1e-6);
        assert!((plan.ground_offset_y - 0.45).abs() < 1e-6);
        assert!((plan.predicted_height() - 1.8).abs() < 1e-6);
    }
}
