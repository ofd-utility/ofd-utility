//! 基础数据类型（见 GB/T 33190—2016 7.3 表 2）。
//!
//! 本标准定义了 6 种基本数据类型：`ST_Loc`、`ST_Array`、`ST_ID`、
//! `ST_RefID`、`ST_Pos`、`ST_Box`。它们在 XML 中均以字符串形式出现，
//! 因此这里统一通过 [`FromStr`] / [`fmt::Display`] 完成与字符串的互转，
//! 并据此实现 serde 的反序列化与序列化。

use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::error::OfdError;

/// 借助 [`fmt::Display`] / [`FromStr`] 为标量基础类型实现 serde。
macro_rules! impl_str_serde {
    ($t:ty) => {
        impl Serialize for $t {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                raw.parse().map_err(de::Error::custom)
            }
        }
    };
}

/// `ST_Loc`：包结构内文件的路径。
///
/// 约定（见表 2）：
/// 1. `/` 代表根节点；
/// 2. 未显式指定时代表当前路径；
/// 3. 路径区分大小写。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct StLoc(pub String);

impl StLoc {
    /// 以字符串切片形式返回路径原文。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 是否为以 `/` 开头的绝对路径（相对于包根节点）。
    pub fn is_absolute(&self) -> bool {
        self.0.starts_with('/')
    }

    /// 将本路径相对 `base_dir`（目录，不含文件名）解析为包内的规范条目名。
    ///
    /// 返回值不含前导 `/`，并已处理 `.` 与 `..`，可直接用于 ZIP 条目查找。
    pub fn resolve(&self, base_dir: &str) -> String {
        resolve_path(base_dir, self)
    }
}

impl fmt::Display for StLoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for StLoc {
    type Err = OfdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(StLoc(s.to_string()))
    }
}

impl From<&str> for StLoc {
    fn from(s: &str) -> Self {
        StLoc(s.to_string())
    }
}

impl_str_serde!(StLoc);

/// `ST_ID`：标识，无符号整数，应在文档内唯一。`0` 表示无效标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StId(pub u64);

impl StId {
    /// 无效标识（取值 `0`）。
    pub const INVALID: StId = StId(0);

    /// 标识是否有效（非 `0`）。
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }

    /// 返回底层数值。
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for StId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for StId {
    type Err = OfdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(StId(parse_id_u64(s)))
    }
}

impl_str_serde!(StId);

/// `ST_RefID`：标识引用，应为文档内已定义的 [`StId`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StRefId(pub u64);

impl StRefId {
    /// 返回底层数值。
    pub fn value(&self) -> u64 {
        self.0
    }

    /// 转换为对应的 [`StId`]。
    pub fn as_id(&self) -> StId {
        StId(self.0)
    }
}

impl fmt::Display for StRefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for StRefId {
    type Err = OfdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(StRefId(parse_id_u64(s)))
    }
}

/// 将标识字符串解析为 `u64`。
///
/// 规范要求 `ST_ID`/`ST_RefID` 为无符号整数，但部分生产工具会写出非整数标识
/// （如 `"999ewm"`）。为避免整篇文档因个别非法标识而解析失败，这里对非整数值
/// 回退为该字符串的稳定哈希：同一文档内的定义与引用使用相同字面量，哈希一致，
/// 仍能正确匹配。哈希值最高位置 1，使其落在 `[2^63, 2^64)`，远离真实的小整数
/// 标识，几乎不会发生碰撞。
fn parse_id_u64(s: &str) -> u64 {
    let trimmed = s.trim();
    match trimmed.parse::<u64>() {
        Ok(v) => v,
        Err(_) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            trimmed.hash(&mut hasher);
            hasher.finish() | (1 << 63)
        }
    }
}

impl_str_serde!(StRefId);

/// `ST_Pos`：点坐标，以空格分割，前者为 x 值，后者为 y 值。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StPos {
    /// 横坐标。
    pub x: f64,
    /// 纵坐标。
    pub y: f64,
}

impl StPos {
    /// 构造一个点坐标。
    pub fn new(x: f64, y: f64) -> Self {
        StPos { x, y }
    }
}

impl fmt::Display for StPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.x, self.y)
    }
}

impl FromStr for StPos {
    type Err = OfdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = parse_floats(s, 2, "ST_Pos")?;
        Ok(StPos { x: v[0], y: v[1] })
    }
}

impl_str_serde!(StPos);

/// `ST_Box`：矩形区域，以空格分割。
///
/// 前两个值代表该矩形左上角的坐标，后两个值依次表示矩形的宽和高，
/// 后两个值应大于 0（见表 2）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StBox {
    /// 左上角横坐标。
    pub x: f64,
    /// 左上角纵坐标。
    pub y: f64,
    /// 宽度（应大于 0）。
    pub width: f64,
    /// 高度（应大于 0）。
    pub height: f64,
}

impl StBox {
    /// 构造一个矩形区域。
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        StBox {
            x,
            y,
            width,
            height,
        }
    }
}

impl fmt::Display for StBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {} {}", self.x, self.y, self.width, self.height)
    }
}

impl FromStr for StBox {
    type Err = OfdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = parse_floats(s, 4, "ST_Box")?;
        if v[2] <= 0.0 || v[3] <= 0.0 {
            return Err(OfdError::BasicType {
                ty: "ST_Box",
                value: s.to_string(),
                reason: "width and height must be greater than 0".to_string(),
            });
        }
        Ok(StBox {
            x: v[0],
            y: v[1],
            width: v[2],
            height: v[3],
        })
    }
}

impl_str_serde!(StBox);

/// `ST_Array`：数组，以空格分割元素；元素不可为 `ST_Loc`/`ST_Array`，不可嵌套。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StArray<T>(pub Vec<T>);

impl<T> StArray<T> {
    /// 以切片形式返回元素。
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// 元素个数。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 是否为空数组。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T: fmt::Display> fmt::Display for StArray<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, item) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{item}")?;
        }
        Ok(())
    }
}

impl<T> FromStr for StArray<T>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    type Err = OfdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut items = Vec::new();
        for token in s.split_whitespace() {
            let item = token.parse().map_err(|e| OfdError::BasicType {
                ty: "ST_Array",
                value: s.to_string(),
                reason: format!("{e}"),
            })?;
            items.push(item);
        }
        Ok(StArray(items))
    }
}

impl<T: fmt::Display> Serialize for StArray<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de, T> Deserialize<'de> for StArray<T>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(de::Error::custom)
    }
}

/// 解析固定数量的浮点数，供 [`StPos`] / [`StBox`] 使用。
fn parse_floats(s: &str, expected: usize, ty: &'static str) -> Result<Vec<f64>, OfdError> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() != expected {
        return Err(OfdError::BasicType {
            ty,
            value: s.to_string(),
            reason: format!("expected {expected} numbers, got {}", tokens.len()),
        });
    }
    let mut values = Vec::with_capacity(expected);
    for token in tokens {
        let value = token.parse::<f64>().map_err(|e| OfdError::BasicType {
            ty,
            value: s.to_string(),
            reason: e.to_string(),
        })?;
        values.push(value);
    }
    Ok(values)
}

/// 返回路径的父目录（不含末尾 `/`），不含目录时返回空串。
pub fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(idx) => &path[..idx],
        None => "",
    }
}

/// 将 [`StLoc`] 相对 `base_dir` 解析为包内规范条目名。
///
/// - 绝对路径（以 `/` 开头）相对包根节点解析，忽略 `base_dir`；
/// - 相对路径相对 `base_dir` 解析；
/// - 处理 `.`（当前路径）与 `..`（父路径）；
/// - 结果不含前导 `/`。
pub fn resolve_path(base_dir: &str, loc: &StLoc) -> String {
    let raw = loc.as_str();
    let mut segments: Vec<&str> = Vec::new();

    if !raw.starts_with('/') {
        for seg in base_dir.split('/').filter(|s| !s.is_empty()) {
            segments.push(seg);
        }
    }

    for seg in raw.split('/').filter(|s| !s.is_empty()) {
        match seg {
            "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }

    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_box() {
        let b: StBox = "10 10 50 50".parse().unwrap();
        assert_eq!(b, StBox::new(10.0, 10.0, 50.0, 50.0));
        assert!("10 10 0 50".parse::<StBox>().is_err());
        assert!("10 10 50".parse::<StBox>().is_err());
    }

    #[test]
    fn parse_pos_and_array() {
        let p: StPos = "12.05 0".parse().unwrap();
        assert_eq!(p, StPos::new(12.05, 0.0));
        let a: StArray<f64> = "12.05 0".parse().unwrap();
        assert_eq!(a.as_slice(), &[12.05, 0.0]);
    }

    #[test]
    fn id_validity() {
        assert!(!StId::INVALID.is_valid());
        assert!(StId(1000).is_valid());
    }

    #[test]
    fn non_numeric_id_falls_back_to_hash() {
        // 合法整数照常解析。
        assert_eq!("55001".parse::<StId>().unwrap().value(), 55001);
        // 非整数标识不再报错，而是回退为稳定哈希且高位置位。
        let a = "999ewm".parse::<StId>().unwrap();
        assert!(a.value() >= 1 << 63, "hashed id should occupy high range");
        // 同一字面量解析一致，保证定义与引用可匹配。
        let b: StRefId = "999ewm".parse().unwrap();
        assert_eq!(a.value(), b.value());
        // 不同字面量得到不同标识。
        assert_ne!(a.value(), "998ewm".parse::<StId>().unwrap().value());
    }

    #[test]
    fn resolve_paths() {
        assert_eq!(resolve_path("", &"OFD.xml".into()), "OFD.xml");
        assert_eq!(
            resolve_path("", &"Doc_0/Document.xml".into()),
            "Doc_0/Document.xml"
        );
        assert_eq!(
            resolve_path("Doc_0", &"Pages/Page_0/Content.xml".into()),
            "Doc_0/Pages/Page_0/Content.xml"
        );
        // 绝对路径忽略 base
        assert_eq!(
            resolve_path("Doc_0", &"/Doc_0/Res/a.png".into()),
            "Doc_0/Res/a.png"
        );
        // 处理 ..
        assert_eq!(
            resolve_path("Doc_0/Pages/Page_0", &"../../Res/a.png".into()),
            "Doc_0/Res/a.png"
        );
    }
}
