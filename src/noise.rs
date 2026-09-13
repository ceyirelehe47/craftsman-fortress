//! 确定性噪声：整数哈希（splitmix64 终结器）驱动的 value noise 与分形叠加。
//!
//! 设计目标：
//! - 相同 Seed + 相同坐标 + 相同生成版本 => 完全一致的结果；
//! - 只使用整数混合与 IEEE 基本浮点运算（+ - * / abs/floor/lerp/smoothstep），
//!   同一构建上逐位确定；
//! - 不依赖任何平台随机源或全局状态，纯函数。

/// splitmix64 终结器混合。
#[inline]
pub fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 二维整数格点哈希 -> `[0, 1)`。
#[inline]
pub fn hash2(seed: u64, x: i64, y: i64) -> f32 {
    let h = mix64(
        seed ^ mix64(
            (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
        ),
    );
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// 三维整数格点哈希 -> `[0, 1)`。
#[inline]
pub fn hash3(seed: u64, x: i64, y: i64, z: i64) -> f32 {
    let h = mix64(
        seed ^ mix64((x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
            ^ mix64((y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
            ^ mix64((z as u64).wrapping_mul(0x27D4_EB4F_1656_6759)),
    );
    (h >> 40) as f32 / (1u64 << 24) as f32
}

#[inline]
fn fade(t: f32) -> f32 {
    // 五次 smoothstep（仅乘加，确定性）。
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 二维 value noise，返回 `[0, 1)`。
pub fn value_noise2(seed: u64, x: f32, z: f32) -> f32 {
    let x0 = x.floor() as i64;
    let z0 = z.floor() as i64;
    let tx = fade(x - x0 as f32);
    let tz = fade(z - z0 as f32);
    let c00 = hash2(seed, x0, z0);
    let c10 = hash2(seed, x0 + 1, z0);
    let c01 = hash2(seed, x0, z0 + 1);
    let c11 = hash2(seed, x0 + 1, z0 + 1);
    lerp(lerp(c00, c10, tx), lerp(c01, c11, tx), tz)
}

/// 三维 value noise，返回 `[0, 1)`。
pub fn value_noise3(seed: u64, x: f32, y: f32, z: f32) -> f32 {
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let z0 = z.floor() as i64;
    let tx = fade(x - x0 as f32);
    let ty = fade(y - y0 as f32);
    let tz = fade(z - z0 as f32);
    let c000 = hash3(seed, x0, y0, z0);
    let c100 = hash3(seed, x0 + 1, y0, z0);
    let c010 = hash3(seed, x0, y0 + 1, z0);
    let c110 = hash3(seed, x0 + 1, y0 + 1, z0);
    let c001 = hash3(seed, x0, y0, z0 + 1);
    let c101 = hash3(seed, x0 + 1, y0, z0 + 1);
    let c011 = hash3(seed, x0, y0 + 1, z0 + 1);
    let c111 = hash3(seed, x0 + 1, y0 + 1, z0 + 1);
    let x00 = lerp(c000, c100, tx);
    let x10 = lerp(c010, c110, tx);
    let x01 = lerp(c001, c101, tx);
    let x11 = lerp(c011, c111, tx);
    lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
}

/// 二维分形叠加（octaves 倍频），返回 `[-1, 1]`。
pub fn fbm2(seed: u64, x: f32, z: f32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut fx = x;
    let mut fz = z;
    let mut s = seed;
    for _ in 0..octaves {
        sum += amp * (value_noise2(s, fx, fz) * 2.0 - 1.0);
        norm += amp;
        amp *= 0.5;
        fx *= 2.0;
        fz *= 2.0;
        s = mix64(s);
    }
    sum / norm
}

/// 三维分形叠加，返回 `[0, 1]`。
pub fn fbm3(seed: u64, x: f32, y: f32, z: f32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut fx = x;
    let mut fy = y;
    let mut fz = z;
    let mut s = seed;
    for _ in 0..octaves {
        sum += amp * value_noise3(s, fx, fy, fz);
        norm += amp;
        amp *= 0.5;
        fx *= 2.0;
        fy *= 2.0;
        fz *= 2.0;
        s = mix64(s);
    }
    sum / norm
}

/// 二维脊状噪声（山体轮廓），返回 `[0, 1]`，值越大越接近山脊。
pub fn ridged2(seed: u64, x: f32, z: f32, octaves: u32) -> f32 {
    let f = fbm2(seed, x, z, octaves); // [-1, 1]
    let r = 1.0 - f.abs(); // [0, 1]
    r * r
}

#[inline]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_deterministic_and_seed_sensitive() {
        assert_eq!(hash2(42, 7, 9), hash2(42, 7, 9));
        assert_ne!(hash2(42, 7, 9), hash2(43, 7, 9));
        assert_ne!(hash2(42, 7, 9), hash2(42, 8, 9));
        assert_eq!(hash3(7, 1, 2, 3), hash3(7, 1, 2, 3));
        assert_ne!(hash3(7, 1, 2, 3), hash3(8, 1, 2, 3));
    }

    #[test]
    fn value_noise_bounded_and_continuous_at_lattice() {
        for s in [1u64, 999] {
            for i in 0..10 {
                for j in 0..10 {
                    let v = value_noise2(s, i as f32, j as f32);
                    assert!((0.0..1.0).contains(&v), "v={v}");
                    let v3 = value_noise3(s, i as f32, j as f32, 3.0);
                    assert!((0.0..1.0).contains(&v3));
                }
            }
        }
    }

    #[test]
    fn fbm_bounded() {
        for i in 0..200 {
            let x = i as f32 * 0.137;
            let v = fbm2(5, x, x * 0.7, 4);
            assert!((-1.0..=1.0).contains(&v), "v={v}");
            let v3 = fbm3(5, x, x * 0.3, x * 0.9, 3);
            assert!((0.0..=1.0).contains(&v3), "v3={v3}");
            let r = ridged2(5, x, x * 0.7, 4);
            assert!((0.0..=1.0).contains(&r), "r={r}");
        }
    }

    /// 确定性：大批量求值两次结果逐位一致。
    #[test]
    fn deterministic_bulk() {
        let mut acc1 = 0u64;
        let mut acc2 = 0u64;
        for i in 0..1000 {
            let x = i as f32 * 0.031;
            let v = value_noise2(12345, x, x * 1.7);
            acc1 = mix64(acc1 ^ v.to_bits() as u64);
        }
        for i in 0..1000 {
            let x = i as f32 * 0.031;
            let v = value_noise2(12345, x, x * 1.7);
            acc2 = mix64(acc2 ^ v.to_bits() as u64);
        }
        assert_eq!(acc1, acc2);
    }
}
