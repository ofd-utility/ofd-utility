//! 杂凑算法与编码工具（供签名完整性校验使用）。
//!
//! OFD 数字签名（见 GB/T 33190—2016 第 18 章）在 `Signature.xml` 中以
//! `References/Reference/CheckValue` 给出每个被保护文件的摘要值，摘要算法由
//! `References@CheckMethod` 指定。国内电子发票/版式文件普遍采用国密 **SM3**
//! （OID `1.2.156.10197.1.401`），`CheckValue` 为 SM3 摘要的 Base64 文本。
//!
//! 为避免引入额外的密码学依赖，这里给出自包含的 SM3（GB/T 32905—2016）实现
//! 与标准 Base64 编码，足以完成“重新计算摘要并与 `CheckValue` 比对”的完整性
//! 校验。注意：本模块**不**实现对签名值（`SignedValue.dat`，SM2 签名）的密码学
//! 验签，那需要证书解析与椭圆曲线运算，超出基础库范围。

/// SM3 初始向量 IV（见 GB/T 32905—2016 4.1）。
const IV: [u32; 8] = [
    0x7380_166f, 0x4914_b2b9, 0x1724_42d7, 0xda8a_0600, 0xa96f_30bc, 0x1631_38aa, 0xe38d_ee4d,
    0xb0fb_0e4e,
];

/// 布尔置换 `P0`（见 4.4）。
#[inline]
fn p0(x: u32) -> u32 {
    x ^ x.rotate_left(9) ^ x.rotate_left(17)
}

/// 布尔置换 `P1`（见 4.4），用于消息扩展。
#[inline]
fn p1(x: u32) -> u32 {
    x ^ x.rotate_left(15) ^ x.rotate_left(23)
}

/// 计算输入数据的 SM3 摘要，返回 32 字节结果。
pub fn sm3(data: &[u8]) -> [u8; 32] {
    let mut v = IV;

    // 消息填充（见 4.2）：追加 `0x80`，补 `0`，末尾附加 64 位比特长度。
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // 逐 512 比特分组迭代压缩（见 4.3）。
    for block in msg.chunks_exact(64) {
        compress(&mut v, block);
    }

    let mut out = [0u8; 32];
    for (i, word) in v.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// 对单个 512 比特分组执行压缩函数 `CF`（见 4.3）。
fn compress(v: &mut [u32; 8], block: &[u8]) {
    // 消息扩展：生成 W[0..68] 与 W'[0..64]（见 5.3.2）。
    let mut w = [0u32; 68];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ]);
    }
    for i in 16..68 {
        w[i] = p1(w[i - 16] ^ w[i - 9] ^ w[i - 3].rotate_left(15))
            ^ w[i - 13].rotate_left(7)
            ^ w[i - 6];
    }
    let mut w1 = [0u32; 64];
    for i in 0..64 {
        w1[i] = w[i] ^ w[i + 4];
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *v;

    for j in 0..64 {
        // 常量 Tj：前 16 轮与后 48 轮不同（见 4.3）。
        let t = if j < 16 { 0x79cc_4519u32 } else { 0x7a87_9d8au32 };
        let ss1 = a
            .rotate_left(12)
            .wrapping_add(e)
            .wrapping_add(t.rotate_left(j as u32))
            .rotate_left(7);
        let ss2 = ss1 ^ a.rotate_left(12);
        // 布尔函数 FFj / GGj：前 16 轮为异或，后 48 轮为择多/选择（见 4.4）。
        let (ff, gg) = if j < 16 {
            (a ^ b ^ c, e ^ f ^ g)
        } else {
            ((a & b) | (a & c) | (b & c), (e & f) | (!e & g))
        };
        let tt1 = ff
            .wrapping_add(d)
            .wrapping_add(ss2)
            .wrapping_add(w1[j]);
        let tt2 = gg
            .wrapping_add(h)
            .wrapping_add(ss1)
            .wrapping_add(w[j]);
        d = c;
        c = b.rotate_left(9);
        b = a;
        a = tt1;
        h = g;
        g = f.rotate_left(19);
        f = e;
        e = p0(tt2);
    }

    v[0] ^= a;
    v[1] ^= b;
    v[2] ^= c;
    v[3] ^= d;
    v[4] ^= e;
    v[5] ^= f;
    v[6] ^= g;
    v[7] ^= h;
}

/// 标准 Base64 编码表（RFC 4648）。
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 将字节序列编码为标准 Base64 文本（含 `=` 填充）。
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(B64[b0 >> 2] as char);
        out.push(B64[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 {
            B64[((b1 & 0x0f) << 2) | (b2 >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[b2 & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GB/T 32905—2016 附录 A 标准示例：`abc` 的 SM3 摘要。
    #[test]
    fn sm3_abc() {
        let digest = sm3(b"abc");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0"
        );
    }

    /// 国标第二个示例：64 字节 `abcd...abcd` 的 SM3 摘要。
    #[test]
    fn sm3_long() {
        let data = b"abcd".repeat(16);
        let digest = sm3(&data);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "debe9ff92275b8a138604889c18e5a4d6fdb70e5387e5765293dcba39c0c5732"
        );
    }

    #[test]
    fn base64_basic() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    /// SM3 摘要的 Base64 文本即为 OFD `CheckValue` 的取值形式。
    #[test]
    fn checkvalue_form() {
        // 与真实 OFD 样张一致：base64(sm3(bytes)) 长度为 44 且以 `=` 结尾。
        let cv = base64_encode(&sm3(b"hello"));
        assert_eq!(cv.len(), 44);
        assert!(cv.ends_with('='));
    }
}
