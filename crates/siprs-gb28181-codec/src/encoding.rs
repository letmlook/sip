//! GB28181 国标编码解析与生成
//!
//! 实现 [`DeviceId`] 的解析、验证、生成以及 SIP URI 构建等功能。

use std::fmt;
use std::str::FromStr;

use crate::types::{CodecError, DeviceId, DeviceType, Industry, ParsedDeviceId, Province};

/// 编码总长度
const CODE_LENGTH: usize = 20;

/// 各字段在字符串中的字节偏移（0-indexed）
mod offset {
    /// 省份编码：第 1-2 位
    pub const PROVINCE: std::ops::Range<usize> = 0..2;
    /// 部门编码：第 3-4 位
    pub const DEPARTMENT: std::ops::Range<usize> = 2..4;
    /// 行业编码：第 5-8 位（前 2 位为行业类型，后 2 位为行业子类）
    pub const INDUSTRY: std::ops::Range<usize> = 4..8;
    /// 设备类型编码：第 9-11 位
    pub const DEVICE_TYPE: std::ops::Range<usize> = 8..11;
    /// 行业扩展编码：第 12-14 位
    pub const INDUSTRY_EXT: std::ops::Range<usize> = 11..14;
    /// 序号：第 15-20 位
    pub const SERIAL: std::ops::Range<usize> = 14..20;
}

impl DeviceId {
    /// 从字符串解析国标编码
    ///
    /// 对输入进行完整校验：长度、纯数字、省份编码、行业编码、设备类型编码。
    ///
    /// # 错误
    ///
    /// - [`CodecError::InvalidLength`] — 长度不为 20
    /// - [`CodecError::InvalidDigit`] — 包含非数字字符
    /// - [`CodecError::InvalidProvince`] — 省份编码不在 11-65 范围内
    /// - [`CodecError::InvalidIndustry`] — 行业编码无法识别
    /// - [`CodecError::InvalidDeviceType`] — 设备类型编码无法识别
    ///
    /// # 示例
    ///
    /// ```
    /// use siprs_gb28181_codec::DeviceId;
    ///
    /// let id = DeviceId::parse("34020000001320000001")?;
    /// assert_eq!(id.as_str(), "34020000001320000001");
    /// # Ok::<(), siprs_gb28181_codec::CodecError>(())
    /// ```
    pub fn parse(s: &str) -> Result<Self, CodecError> {
        // 1. 长度校验
        if s.len() != CODE_LENGTH {
            return Err(CodecError::InvalidLength(s.len()));
        }

        // 2. 纯数字校验
        for (i, ch) in s.char_indices() {
            if !ch.is_ascii_digit() {
                return Err(CodecError::InvalidDigit {
                    position: i + 1,
                    digit: ch,
                });
            }
        }

        // 3. 省份编码校验
        let province_code = &s[offset::PROVINCE];
        if Province::from_code(province_code).is_none() {
            return Err(CodecError::InvalidProvince(province_code.to_string()));
        }

        // 4. 行业编码校验（前 2 位）
        let industry_code = &s[offset::INDUSTRY];
        let industry_prefix = &industry_code[..2];
        if Industry::from_code(industry_prefix).is_none() {
            return Err(CodecError::InvalidIndustry(industry_code.to_string()));
        }

        // 5. 设备类型编码校验
        let device_type_code = &s[offset::DEVICE_TYPE];
        if DeviceType::from_code(device_type_code).is_none() {
            return Err(CodecError::InvalidDeviceType(device_type_code.to_string()));
        }

        Ok(DeviceId(s.to_string()))
    }

    /// 从各组成部分生成国标编码
    ///
    /// # 参数
    ///
    /// - `province` — 省份
    /// - `department` — 部门编码 (0-99)
    /// - `industry` — 行业
    /// - `device_type` — 设备类型
    /// - `industry_extension` — 行业扩展编码 (0-999)
    /// - `serial_number` — 序号 (1-999999)
    ///
    /// # 错误
    ///
    /// 当参数超出合法范围时返回相应错误。
    ///
    /// # 示例
    ///
    /// ```
    /// use siprs_gb28181_codec::{DeviceId, DeviceType, Industry, Province};
    ///
    /// let id = DeviceId::compose(
    ///     Province::Anhui,
    ///     2,
    ///     Industry::SocialSecurity,
    ///     DeviceType::FixedCamera,
    ///     0,
    ///     1,
    /// )?;
    /// assert_eq!(id.as_str(), "34020200113000000001");
    /// # Ok::<(), siprs_gb28181_codec::CodecError>(())
    /// ```
    pub fn compose(
        province: Province,
        department: u8,
        industry: Industry,
        device_type: DeviceType,
        industry_extension: u16,
        serial_number: u32,
    ) -> Result<Self, CodecError> {
        // 参数范围校验
        if department > 99 {
            return Err(CodecError::InvalidDepartment(format!(
                "{department}: must be 0-99"
            )));
        }
        if industry_extension > 999 {
            return Err(CodecError::InvalidIndustryExtension(format!(
                "{industry_extension}: must be 0-999"
            )));
        }
        if serial_number == 0 || serial_number > 999999 {
            return Err(CodecError::InvalidSerialNumber(format!(
                "{serial_number}: must be 1-999999"
            )));
        }

        // 行业编码：2 位行业类型 + "00" 补齐为 4 位
        let industry_str = format!("{}00", industry.code());

        let s = format!(
            "{}{:02}{}{}{:03}{:06}",
            province.code(),
            department,
            industry_str,
            device_type.code(),
            industry_extension,
            serial_number,
        );

        debug_assert_eq!(
            s.len(),
            CODE_LENGTH,
            "compose produced {}-char string: {s}",
            s.len()
        );
        Ok(DeviceId(s))
    }

    /// 解析为结构化数据
    ///
    /// 将 20 位编码拆解为各组成部分。
    ///
    /// # 示例
    ///
    /// ```
    /// use siprs_gb28181_codec::{DeviceId, DeviceType, Industry, Province};
    ///
    /// let id = DeviceId::parse("34020200113000000001")?;
    /// let parsed = id.decode()?;
    /// assert_eq!(parsed.province, Province::Anhui);
    /// assert_eq!(parsed.department, 2);
    /// assert_eq!(parsed.industry, Industry::SocialSecurity);
    /// assert_eq!(parsed.device_type, DeviceType::FixedCamera);
    /// # Ok::<(), siprs_gb28181_codec::CodecError>(())
    /// ```
    pub fn decode(&self) -> Result<ParsedDeviceId, CodecError> {
        let s = &self.0;

        let province = Province::from_code(&s[offset::PROVINCE])
            .ok_or_else(|| CodecError::InvalidProvince(s[offset::PROVINCE].to_string()))?;

        let department: u8 = s[offset::DEPARTMENT]
            .parse()
            .map_err(|_| CodecError::InvalidDepartment(s[offset::DEPARTMENT].to_string()))?;

        let industry_raw = s[offset::INDUSTRY].to_string();
        let industry = Industry::from_code(&industry_raw[..2])
            .ok_or_else(|| CodecError::InvalidIndustry(industry_raw.clone()))?;

        let device_type = DeviceType::from_code(&s[offset::DEVICE_TYPE])
            .ok_or_else(|| CodecError::InvalidDeviceType(s[offset::DEVICE_TYPE].to_string()))?;

        let industry_extension: u16 = s[offset::INDUSTRY_EXT].parse().map_err(|_| {
            CodecError::InvalidIndustryExtension(s[offset::INDUSTRY_EXT].to_string())
        })?;

        let serial_number: u32 = s[offset::SERIAL]
            .parse()
            .map_err(|_| CodecError::InvalidSerialNumber(s[offset::SERIAL].to_string()))?;

        Ok(ParsedDeviceId {
            province,
            department,
            industry,
            industry_raw,
            device_type,
            industry_extension,
            serial_number,
        })
    }

    /// 获取原始字符串引用
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 验证编码格式是否合法
    ///
    /// 已通过 [`DeviceId::parse`] 构造的实例一定合法；
    /// 此方法主要用于从其他途径获得的 `DeviceId` 做二次校验。
    pub fn is_valid(&self) -> bool {
        self.decode().is_ok()
    }

    /// 获取省份编码（2 位字符串）
    pub fn province_code(&self) -> &str {
        &self.0[offset::PROVINCE]
    }

    /// 获取行业编码（4 位字符串）
    pub fn industry_code(&self) -> &str {
        &self.0[offset::INDUSTRY]
    }

    /// 获取设备类型编码（3 位字符串）
    pub fn device_type_code(&self) -> &str {
        &self.0[offset::DEVICE_TYPE]
    }

    /// 获取序号
    pub fn serial_number(&self) -> u32 {
        self.0[offset::SERIAL].parse().unwrap_or(0)
    }

    /// 构建用于 SIP URI 的格式
    ///
    /// 返回 `sip:{device_id}@{host}:{port}`
    ///
    /// # 示例
    ///
    /// ```
    /// use siprs_gb28181_codec::DeviceId;
    ///
    /// let id = DeviceId::parse("34020000001320000001")?;
    /// let uri = id.to_sip_uri("192.168.1.1", 5060);
    /// assert_eq!(uri, "sip:34020000001320000001@192.168.1.1:5060");
    /// # Ok::<(), siprs_gb28181_codec::CodecError>(())
    /// ```
    pub fn to_sip_uri(&self, host: &str, port: u16) -> String {
        format!("sip:{}@{}:{}", self.0, host, port)
    }

    /// 获取部门编码数值
    pub fn department(&self) -> u8 {
        self.0[offset::DEPARTMENT].parse().unwrap_or(0)
    }

    /// 获取行业扩展编码数值
    pub fn industry_extension(&self) -> u16 {
        self.0[offset::INDUSTRY_EXT].parse().unwrap_or(0)
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for DeviceId {
    type Err = CodecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DeviceId::parse(s)
    }
}

impl AsRef<str> for DeviceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── 常用测试编码 ─────────────────────────────────────────────────
    // 安徽/02/社会治安(0200)/固定枪机(113)/000/000001
    const TEST_ID_FIXED: &str = "34020200113000000001";
    // 经典示例：安徽/02/社会治安(0000)/Other(013)/200/000001
    const TEST_ID_CLASSIC: &str = "34020000001320000001";

    // ── 解析测试 ──────────────────────────────────────────────────────

    #[test]
    fn test_parse_valid() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert_eq!(id.as_str(), TEST_ID_FIXED);
    }

    #[test]
    fn test_parse_invalid_length() {
        let err = DeviceId::parse("123").unwrap_err();
        assert!(matches!(err, CodecError::InvalidLength(3)));

        let err = DeviceId::parse("340202001130000000011").unwrap_err();
        assert!(matches!(err, CodecError::InvalidLength(21)));
    }

    #[test]
    fn test_parse_invalid_digit() {
        let err = DeviceId::parse("34A20200113000000001").unwrap_err();
        assert!(matches!(
            err,
            CodecError::InvalidDigit {
                position: 3,
                digit: 'A'
            }
        ));
    }

    #[test]
    fn test_parse_invalid_province() {
        let err = DeviceId::parse("99020200113000000001").unwrap_err();
        assert!(matches!(err, CodecError::InvalidProvince(_)));
        if let CodecError::InvalidProvince(code) = err {
            assert_eq!(code, "99");
        }
    }

    #[test]
    fn test_parse_classic_example() {
        // 经典示例编码 "34020000001320000001"
        // 按字段拆分：34|02|0000|001|320|000001
        let id = DeviceId::parse(TEST_ID_CLASSIC).unwrap();
        let parsed = id.decode().unwrap();

        assert_eq!(parsed.province, Province::Anhui);
        assert_eq!(parsed.department, 2);
        assert_eq!(parsed.industry, Industry::SocialSecurity); // "00" 前缀 -> 社会治安
        assert_eq!(parsed.industry_raw, "0000");
        // 位置 8-11 为 "001"，不属于标准设备类型，映射为 Other(1)
        assert_eq!(parsed.device_type, DeviceType::Other(1));
        assert_eq!(parsed.industry_extension, 320);
        assert_eq!(parsed.serial_number, 1);
    }

    // ── 解码测试 ──────────────────────────────────────────────────────

    #[test]
    fn test_decode_standard() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        let parsed = id.decode().unwrap();

        assert_eq!(parsed.province, Province::Anhui);
        assert_eq!(parsed.department, 2);
        assert_eq!(parsed.industry, Industry::SocialSecurity);
        assert_eq!(parsed.industry_raw, "0200");
        assert_eq!(parsed.device_type, DeviceType::FixedCamera); // 113
        assert_eq!(parsed.industry_extension, 0);
        assert_eq!(parsed.serial_number, 1);
    }

    // ── 生成测试 ──────────────────────────────────────────────────────

    #[test]
    fn test_compose() {
        let id = DeviceId::compose(
            Province::Anhui,
            2,
            Industry::SocialSecurity,
            DeviceType::FixedCamera,
            0,
            1,
        )
        .unwrap();
        assert_eq!(id.as_str(), "34020200113000000001");
    }

    #[test]
    fn test_compose_decoder() {
        let id = DeviceId::compose(
            Province::Anhui,
            2,
            Industry::SocialSecurity,
            DeviceType::Decoder,
            0,
            1,
        )
        .unwrap();
        assert_eq!(id.as_str(), "34020200130000000001");
    }

    #[test]
    fn test_compose_roundtrip() {
        // compose -> decode -> compose 应该得到相同结果
        let id = DeviceId::compose(
            Province::Beijing,
            1,
            Industry::PublicSecurity,
            DeviceType::SphericalCamera,
            123,
            456789,
        )
        .unwrap();

        let parsed = id.decode().unwrap();
        let id2 = DeviceId::compose(
            parsed.province,
            parsed.department,
            parsed.industry,
            parsed.device_type,
            parsed.industry_extension,
            parsed.serial_number,
        )
        .unwrap();

        assert_eq!(id, id2);
    }

    #[test]
    fn test_parse_compose_roundtrip() {
        // parse -> decode -> compose 应该得到语义一致的编码
        let original = TEST_ID_FIXED;
        let id = DeviceId::parse(original).unwrap();
        let parsed = id.decode().unwrap();

        let id2 = DeviceId::compose(
            parsed.province,
            parsed.department,
            parsed.industry,
            parsed.device_type,
            parsed.industry_extension,
            parsed.serial_number,
        )
        .unwrap();

        assert_eq!(id2.as_str(), original);
    }

    #[test]
    fn test_compose_invalid_department() {
        let err = DeviceId::compose(
            Province::Beijing,
            100,
            Industry::SocialSecurity,
            DeviceType::FixedCamera,
            0,
            1,
        )
        .unwrap_err();
        assert!(matches!(err, CodecError::InvalidDepartment(_)));
    }

    #[test]
    fn test_compose_invalid_serial() {
        let err = DeviceId::compose(
            Province::Beijing,
            1,
            Industry::SocialSecurity,
            DeviceType::FixedCamera,
            0,
            0,
        )
        .unwrap_err();
        assert!(matches!(err, CodecError::InvalidSerialNumber(_)));

        let err = DeviceId::compose(
            Province::Beijing,
            1,
            Industry::SocialSecurity,
            DeviceType::FixedCamera,
            0,
            1000000,
        )
        .unwrap_err();
        assert!(matches!(err, CodecError::InvalidSerialNumber(_)));
    }

    #[test]
    fn test_compose_invalid_extension() {
        let err = DeviceId::compose(
            Province::Beijing,
            1,
            Industry::SocialSecurity,
            DeviceType::FixedCamera,
            1000,
            1,
        )
        .unwrap_err();
        assert!(matches!(err, CodecError::InvalidIndustryExtension(_)));
    }

    #[test]
    fn test_compose_max_values() {
        let id = DeviceId::compose(
            Province::Xinjiang,
            99,
            Industry::Other,
            DeviceType::Platform,
            999,
            999999,
        )
        .unwrap();
        // 65 + 99 + 0800 + 170 + 999 + 999999
        assert_eq!(id.as_str(), "65990800170999999999");

        let parsed = id.decode().unwrap();
        assert_eq!(parsed.province, Province::Xinjiang);
        assert_eq!(parsed.department, 99);
        assert_eq!(parsed.industry, Industry::Other);
        assert_eq!(parsed.device_type, DeviceType::Platform);
        assert_eq!(parsed.industry_extension, 999);
        assert_eq!(parsed.serial_number, 999999);
    }

    // ── SIP URI 测试 ──────────────────────────────────────────────────

    #[test]
    fn test_sip_uri() {
        let id = DeviceId::parse(TEST_ID_CLASSIC).unwrap();
        let uri = id.to_sip_uri("192.168.1.1", 5060);
        assert_eq!(uri, "sip:34020000001320000001@192.168.1.1:5060");
    }

    // ── 字段访问器测试 ────────────────────────────────────────────────

    #[test]
    fn test_province_code_accessor() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert_eq!(id.province_code(), "34");
    }

    #[test]
    fn test_industry_code_accessor() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert_eq!(id.industry_code(), "0200");
    }

    #[test]
    fn test_device_type_code_accessor() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert_eq!(id.device_type_code(), "113");
    }

    #[test]
    fn test_serial_number_accessor() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert_eq!(id.serial_number(), 1);
    }

    #[test]
    fn test_department_accessor() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert_eq!(id.department(), 2);
    }

    #[test]
    fn test_industry_extension_accessor() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert_eq!(id.industry_extension(), 0);
    }

    // ── trait 实现 ────────────────────────────────────────────────────

    #[test]
    fn test_display() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert_eq!(format!("{id}"), TEST_ID_FIXED);
    }

    #[test]
    fn test_from_str() {
        let id: DeviceId = TEST_ID_FIXED.parse().unwrap();
        assert_eq!(id.as_str(), TEST_ID_FIXED);
    }

    #[test]
    fn test_is_valid() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert!(id.is_valid());
    }

    #[test]
    fn test_as_ref() {
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        let s: &str = id.as_ref();
        assert_eq!(s, TEST_ID_FIXED);
    }

    #[test]
    fn test_equality() {
        let id1 = DeviceId::parse(TEST_ID_FIXED).unwrap();
        let id2 = DeviceId::parse(TEST_ID_FIXED).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_clone() {
        let id1 = DeviceId::parse(TEST_ID_FIXED).unwrap();
        let id2 = id1.clone();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let id = DeviceId::parse(TEST_ID_FIXED).unwrap();
        set.insert(id.clone());
        assert!(set.contains(&id));
    }

    // ── 枚举遍历测试 ─────────────────────────────────────────────────

    #[test]
    fn test_all_provinces() {
        for province in Province::all() {
            let code = province.code();
            // 省份(2) + 02 + 0200 + 113 + 000 + 000001
            let test_id = format!("{code}020200113000000001");
            assert_eq!(test_id.len(), 20, "Bad test ID for province {code}");
            let id = DeviceId::parse(&test_id);
            assert!(id.is_ok(), "Failed to parse for province {code}: {:?}", id);
            let parsed = id.unwrap().decode().unwrap();
            assert_eq!(parsed.province, *province);
        }
    }

    #[test]
    fn test_all_industries() {
        let test_cases = [
            ("00", Industry::SocialSecurity),
            ("02", Industry::SocialSecurity),
            ("03", Industry::PublicSecurity),
            ("04", Industry::Traffic),
            ("05", Industry::Justice),
            ("06", Industry::FireFighting),
            ("07", Industry::Border),
            ("08", Industry::Other),
        ];

        for (code, expected) in test_cases {
            // 行业编码 4 位：code + "00"
            let industry_4 = format!("{code}00");
            // 34 + 02 + industry_4 + 113 + 000 + 000001
            let test_id = format!("3402{industry_4}113000000001");
            assert_eq!(test_id.len(), 20, "Bad test ID for industry {code}");
            let id = DeviceId::parse(&test_id);
            assert!(id.is_ok(), "Failed to parse for industry {code}: {:?}", id);
            let parsed = id.unwrap().decode().unwrap();
            assert_eq!(
                parsed.industry, expected,
                "Mismatch for industry code {code}"
            );
        }
    }

    #[test]
    fn test_all_device_types() {
        let test_cases = [
            ("111", DeviceType::SphericalCamera),
            ("112", DeviceType::HalfSphericalCamera),
            ("113", DeviceType::FixedCamera),
            ("114", DeviceType::PtzCamera),
            ("121", DeviceType::Encoder),
            ("131", DeviceType::Decoder),
            ("141", DeviceType::AlarmInput),
            ("151", DeviceType::AlarmOutput),
            ("161", DeviceType::NetworkDevice),
            ("171", DeviceType::Platform),
        ];

        for (code, expected) in test_cases {
            // 34 + 02 + 0200 + code + 000 + 000001
            let test_id = format!("34020200{code}000000001");
            assert_eq!(test_id.len(), 20, "Bad test ID for device type {code}");
            let id = DeviceId::parse(&test_id);
            assert!(
                id.is_ok(),
                "Failed to parse for device type {code}: {:?}",
                id
            );
            let parsed = id.unwrap().decode().unwrap();
            assert_eq!(
                parsed.device_type, expected,
                "Mismatch for device type code {code}"
            );
        }
    }

    #[test]
    fn test_device_type_other() {
        // 34 + 02 + 0200 + 199 + 000 + 000001
        let test_id = "34020200199000000001";
        assert_eq!(test_id.len(), 20);
        let id = DeviceId::parse(test_id).unwrap();
        let parsed = id.decode().unwrap();
        assert_eq!(parsed.device_type, DeviceType::Other(199));
    }

    // ── 枚举方法测试 ─────────────────────────────────────────────────

    #[test]
    fn test_province_from_code() {
        assert_eq!(Province::from_code("34"), Some(Province::Anhui));
        assert_eq!(Province::from_code("11"), Some(Province::Beijing));
        assert_eq!(Province::from_code("99"), None);
        assert_eq!(Province::from_code("10"), None);
    }

    #[test]
    fn test_province_code_method() {
        assert_eq!(Province::Anhui.code(), "34");
        assert_eq!(Province::Beijing.code(), "11");
    }

    #[test]
    fn test_province_name() {
        assert_eq!(Province::Anhui.name(), "安徽");
        assert_eq!(Province::Beijing.name(), "北京");
    }

    #[test]
    fn test_industry_from_code() {
        assert_eq!(Industry::from_code("00"), Some(Industry::SocialSecurity));
        assert_eq!(Industry::from_code("02"), Some(Industry::SocialSecurity));
        assert_eq!(Industry::from_code("03"), Some(Industry::PublicSecurity));
        assert_eq!(Industry::from_code("99"), None);
    }

    #[test]
    fn test_industry_code_method() {
        assert_eq!(Industry::SocialSecurity.code(), "02");
        assert_eq!(Industry::PublicSecurity.code(), "03");
    }

    #[test]
    fn test_industry_name() {
        assert_eq!(Industry::SocialSecurity.name(), "社会治安");
        assert_eq!(Industry::PublicSecurity.name(), "公安");
    }

    #[test]
    fn test_device_type_from_code() {
        assert_eq!(
            DeviceType::from_code("111"),
            Some(DeviceType::SphericalCamera)
        );
        assert_eq!(DeviceType::from_code("113"), Some(DeviceType::FixedCamera));
        assert_eq!(DeviceType::from_code("121"), Some(DeviceType::Encoder));
        assert_eq!(DeviceType::from_code("199"), Some(DeviceType::Other(199)));
        assert_eq!(DeviceType::from_code("11"), None);
    }

    #[test]
    fn test_device_type_code_method() {
        assert_eq!(DeviceType::SphericalCamera.code(), "111");
        assert_eq!(DeviceType::FixedCamera.code(), "113");
        assert_eq!(DeviceType::Encoder.code(), "120");
        assert_eq!(DeviceType::Other(199).code(), "199");
    }

    #[test]
    fn test_device_type_name() {
        assert_eq!(DeviceType::SphericalCamera.name(), "球机");
        assert_eq!(DeviceType::FixedCamera.name(), "固定枪机");
        assert_eq!(DeviceType::Other(999).name(), "其他");
    }

    #[test]
    fn test_debug_classic_string_indexing() {
        let s = "34020000001320000001";
        assert_eq!(s.len(), 20, "String length should be 20");
        assert_eq!(&s[0..2], "34", "Province");
        assert_eq!(&s[2..4], "02", "Department");
        assert_eq!(&s[4..8], "0000", "Industry");
        assert_eq!(&s[8..11], "001", "Device type");
        assert_eq!(&s[11..14], "320", "Industry extension");
        assert_eq!(&s[14..20], "000001", "Serial");
        assert_eq!(DeviceType::from_code("001"), Some(DeviceType::Other(1)));
    }
}
