//! GB28181 国标编码类型定义
//!
//! 20 位编码格式：`AABBCCDDDDEEEFFF`
//!
//! | 位置  | 长度 | 名称         | 说明                                                                 |
//! |-------|------|--------------|----------------------------------------------------------------------|
//! | 1-2   | 2    | 省份编码     | 11-65（行政区划前2位）                                               |
//! | 3-4   | 2    | 部门编码     | 00-99                                                                |
//! | 5-8   | 4    | 行业编码     | 02=社会治安，03=公安，04=交通，05=司法，06=消防，07=边防，08=其他   |
//! | 9-11  | 3    | 设备类型编码 | 111=球机，112=半球，113=固定枪机，114=遥控枪机，12x=编码设备，...   |
//! | 12-14 | 3    | 行业扩展编码 | 000-999                                                              |
//! | 15-20 | 6    | 序号         | 000001-999999                                                        |

/// GB28181 20 位国标编码
///
/// 包装一个 20 位数字字符串，保证经过验证后内容合法。
/// 内部字段为 `pub(crate)` 以便在同一 crate 内的模块中访问。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(pub(crate) String);

/// 省份编码（行政区划前 2 位）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Province {
    /// 11 - 北京
    Beijing,
    /// 12 - 天津
    Tianjin,
    /// 13 - 河北
    Hebei,
    /// 14 - 山西
    Shanxi,
    /// 15 - 内蒙古
    InnerMongolia,
    /// 21 - 辽宁
    Liaoning,
    /// 22 - 吉林
    Jilin,
    /// 23 - 黑龙江
    Heilongjiang,
    /// 31 - 上海
    Shanghai,
    /// 32 - 江苏
    Jiangsu,
    /// 33 - 浙江
    Zhejiang,
    /// 34 - 安徽
    Anhui,
    /// 35 - 福建
    Fujian,
    /// 36 - 江西
    Jiangxi,
    /// 37 - 山东
    Shandong,
    /// 41 - 河南
    Henan,
    /// 42 - 湖北
    Hubei,
    /// 43 - 湖南
    Hunan,
    /// 44 - 广东
    Guangdong,
    /// 45 - 广西
    Guangxi,
    /// 46 - 海南
    Hainan,
    /// 50 - 重庆
    Chongqing,
    /// 51 - 四川
    Sichuan,
    /// 52 - 贵州
    Guizhou,
    /// 53 - 云南
    Yunnan,
    /// 54 - 西藏
    Tibet,
    /// 61 - 陕西
    Shaanxi,
    /// 62 - 甘肃
    Gansu,
    /// 63 - 青海
    Qinghai,
    /// 64 - 宁夏
    Ningxia,
    /// 65 - 新疆
    Xinjiang,
}

impl Province {
    /// 从 2 位数字代码解析省份
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "11" => Some(Province::Beijing),
            "12" => Some(Province::Tianjin),
            "13" => Some(Province::Hebei),
            "14" => Some(Province::Shanxi),
            "15" => Some(Province::InnerMongolia),
            "21" => Some(Province::Liaoning),
            "22" => Some(Province::Jilin),
            "23" => Some(Province::Heilongjiang),
            "31" => Some(Province::Shanghai),
            "32" => Some(Province::Jiangsu),
            "33" => Some(Province::Zhejiang),
            "34" => Some(Province::Anhui),
            "35" => Some(Province::Fujian),
            "36" => Some(Province::Jiangxi),
            "37" => Some(Province::Shandong),
            "41" => Some(Province::Henan),
            "42" => Some(Province::Hubei),
            "43" => Some(Province::Hunan),
            "44" => Some(Province::Guangdong),
            "45" => Some(Province::Guangxi),
            "46" => Some(Province::Hainan),
            "50" => Some(Province::Chongqing),
            "51" => Some(Province::Sichuan),
            "52" => Some(Province::Guizhou),
            "53" => Some(Province::Yunnan),
            "54" => Some(Province::Tibet),
            "61" => Some(Province::Shaanxi),
            "62" => Some(Province::Gansu),
            "63" => Some(Province::Qinghai),
            "64" => Some(Province::Ningxia),
            "65" => Some(Province::Xinjiang),
            _ => None,
        }
    }

    /// 获取省份对应的 2 位数字代码
    pub fn code(self) -> &'static str {
        match self {
            Province::Beijing => "11",
            Province::Tianjin => "12",
            Province::Hebei => "13",
            Province::Shanxi => "14",
            Province::InnerMongolia => "15",
            Province::Liaoning => "21",
            Province::Jilin => "22",
            Province::Heilongjiang => "23",
            Province::Shanghai => "31",
            Province::Jiangsu => "32",
            Province::Zhejiang => "33",
            Province::Anhui => "34",
            Province::Fujian => "35",
            Province::Jiangxi => "36",
            Province::Shandong => "37",
            Province::Henan => "41",
            Province::Hubei => "42",
            Province::Hunan => "43",
            Province::Guangdong => "44",
            Province::Guangxi => "45",
            Province::Hainan => "46",
            Province::Chongqing => "50",
            Province::Sichuan => "51",
            Province::Guizhou => "52",
            Province::Yunnan => "53",
            Province::Tibet => "54",
            Province::Shaanxi => "61",
            Province::Gansu => "62",
            Province::Qinghai => "63",
            Province::Ningxia => "64",
            Province::Xinjiang => "65",
        }
    }

    /// 获取省份中文名称
    pub fn name(self) -> &'static str {
        match self {
            Province::Beijing => "北京",
            Province::Tianjin => "天津",
            Province::Hebei => "河北",
            Province::Shanxi => "山西",
            Province::InnerMongolia => "内蒙古",
            Province::Liaoning => "辽宁",
            Province::Jilin => "吉林",
            Province::Heilongjiang => "黑龙江",
            Province::Shanghai => "上海",
            Province::Jiangsu => "江苏",
            Province::Zhejiang => "浙江",
            Province::Anhui => "安徽",
            Province::Fujian => "福建",
            Province::Jiangxi => "江西",
            Province::Shandong => "山东",
            Province::Henan => "河南",
            Province::Hubei => "湖北",
            Province::Hunan => "湖南",
            Province::Guangdong => "广东",
            Province::Guangxi => "广西",
            Province::Hainan => "海南",
            Province::Chongqing => "重庆",
            Province::Sichuan => "四川",
            Province::Guizhou => "贵州",
            Province::Yunnan => "云南",
            Province::Tibet => "西藏",
            Province::Shaanxi => "陕西",
            Province::Gansu => "甘肃",
            Province::Qinghai => "青海",
            Province::Ningxia => "宁夏",
            Province::Xinjiang => "新疆",
        }
    }

    /// 返回所有省份列表
    pub fn all() -> &'static [Province] {
        &[
            Province::Beijing,
            Province::Tianjin,
            Province::Hebei,
            Province::Shanxi,
            Province::InnerMongolia,
            Province::Liaoning,
            Province::Jilin,
            Province::Heilongjiang,
            Province::Shanghai,
            Province::Jiangsu,
            Province::Zhejiang,
            Province::Anhui,
            Province::Fujian,
            Province::Jiangxi,
            Province::Shandong,
            Province::Henan,
            Province::Hubei,
            Province::Hunan,
            Province::Guangdong,
            Province::Guangxi,
            Province::Hainan,
            Province::Chongqing,
            Province::Sichuan,
            Province::Guizhou,
            Province::Yunnan,
            Province::Tibet,
            Province::Shaanxi,
            Province::Gansu,
            Province::Qinghai,
            Province::Ningxia,
            Province::Xinjiang,
        ]
    }
}

/// 行业编码
///
/// 对应 4 位行业编码字段的前 2 位。
/// - `00` 视为社会治安（社会治安内部接入）
/// - `02` 社会治安
/// - `03` 公安
/// - `04` 交通
/// - `05` 司法
/// - `06` 消防
/// - `07` 边防
/// - `08` 其他
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Industry {
    /// 00/02 - 社会治安
    SocialSecurity,
    /// 03 - 公安
    PublicSecurity,
    /// 04 - 交通
    Traffic,
    /// 05 - 司法
    Justice,
    /// 06 - 消防
    FireFighting,
    /// 07 - 边防
    Border,
    /// 08 - 其他
    Other,
}

impl Industry {
    /// 从行业编码的前 2 位数字解析行业类型
    ///
    /// `code` 应为 2 位数字字符串，如 "02"、"03" 等。
    /// "00" 视为社会治安（社会治安内部接入）。
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "00" | "02" => Some(Industry::SocialSecurity),
            "03" => Some(Industry::PublicSecurity),
            "04" => Some(Industry::Traffic),
            "05" => Some(Industry::Justice),
            "06" => Some(Industry::FireFighting),
            "07" => Some(Industry::Border),
            "08" => Some(Industry::Other),
            _ => None,
        }
    }

    /// 获取行业对应的 2 位数字代码
    pub fn code(self) -> &'static str {
        match self {
            Industry::SocialSecurity => "02",
            Industry::PublicSecurity => "03",
            Industry::Traffic => "04",
            Industry::Justice => "05",
            Industry::FireFighting => "06",
            Industry::Border => "07",
            Industry::Other => "08",
        }
    }

    /// 获取行业中文名称
    pub fn name(self) -> &'static str {
        match self {
            Industry::SocialSecurity => "社会治安",
            Industry::PublicSecurity => "公安",
            Industry::Traffic => "交通",
            Industry::Justice => "司法",
            Industry::FireFighting => "消防",
            Industry::Border => "边防",
            Industry::Other => "其他",
        }
    }
}

/// 设备类型编码（3 位数字）
///
/// - `111` = 球机
/// - `112` = 半球
/// - `113` = 固定枪机
/// - `114` = 遥控枪机
/// - `12x` = 编码设备
/// - `13x` = 解码设备
/// - `14x` = 报警输入设备
/// - `15x` = 报警输出设备
/// - `16x` = 网络设备
/// - `17x` = 平台设备
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    /// 111 - 球机
    SphericalCamera,
    /// 112 - 半球
    HalfSphericalCamera,
    /// 113 - 固定枪机
    FixedCamera,
    /// 114 - 遥控枪机
    PtzCamera,
    /// 12x - 编码设备
    Encoder,
    /// 13x - 解码设备
    Decoder,
    /// 14x - 报警输入设备
    AlarmInput,
    /// 15x - 报警输出设备
    AlarmOutput,
    /// 16x - 网络设备
    NetworkDevice,
    /// 17x - 平台设备
    Platform,
    /// 其他未知设备类型，携带原始 3 位编码
    Other(u16),
}

impl DeviceType {
    /// 从 3 位数字代码解析设备类型
    pub fn from_code(code: &str) -> Option<Self> {
        if code.len() != 3 {
            return None;
        }
        match code {
            "111" => Some(DeviceType::SphericalCamera),
            "112" => Some(DeviceType::HalfSphericalCamera),
            "113" => Some(DeviceType::FixedCamera),
            "114" => Some(DeviceType::PtzCamera),
            c if c.starts_with('1') => match c.as_bytes()[1] {
                b'2' => Some(DeviceType::Encoder),
                b'3' => Some(DeviceType::Decoder),
                b'4' => Some(DeviceType::AlarmInput),
                b'5' => Some(DeviceType::AlarmOutput),
                b'6' => Some(DeviceType::NetworkDevice),
                b'7' => Some(DeviceType::Platform),
                _ => {
                    let v = c.parse::<u16>().ok()?;
                    Some(DeviceType::Other(v))
                }
            },
            c => {
                let v = c.parse::<u16>().ok()?;
                Some(DeviceType::Other(v))
            }
        }
    }

    /// 获取设备类型对应的 3 位数字代码
    pub fn code(self) -> String {
        match self {
            DeviceType::SphericalCamera => "111".to_string(),
            DeviceType::HalfSphericalCamera => "112".to_string(),
            DeviceType::FixedCamera => "113".to_string(),
            DeviceType::PtzCamera => "114".to_string(),
            DeviceType::Encoder => "120".to_string(),
            DeviceType::Decoder => "130".to_string(),
            DeviceType::AlarmInput => "140".to_string(),
            DeviceType::AlarmOutput => "150".to_string(),
            DeviceType::NetworkDevice => "160".to_string(),
            DeviceType::Platform => "170".to_string(),
            DeviceType::Other(v) => format!("{v:03}"),
        }
    }

    /// 获取设备类型中文名称
    pub fn name(self) -> &'static str {
        match self {
            DeviceType::SphericalCamera => "球机",
            DeviceType::HalfSphericalCamera => "半球",
            DeviceType::FixedCamera => "固定枪机",
            DeviceType::PtzCamera => "遥控枪机",
            DeviceType::Encoder => "编码设备",
            DeviceType::Decoder => "解码设备",
            DeviceType::AlarmInput => "报警输入",
            DeviceType::AlarmOutput => "报警输出",
            DeviceType::NetworkDevice => "网络设备",
            DeviceType::Platform => "平台设备",
            DeviceType::Other(_) => "其他",
        }
    }
}

/// 解析后的编码结构
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDeviceId {
    /// 省份
    pub province: Province,
    /// 部门编码 (0-99)
    pub department: u8,
    /// 行业
    pub industry: Industry,
    /// 行业编码的原始 4 位值
    pub industry_raw: String,
    /// 设备类型
    pub device_type: DeviceType,
    /// 行业扩展编码 (0-999)
    pub industry_extension: u16,
    /// 序号 (1-999999)
    pub serial_number: u32,
}

/// 编码错误
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    /// 编码长度无效，期望 20 位
    #[error("invalid length: expected 20, got {0}")]
    InvalidLength(usize),
    /// 某位置存在非数字字符
    #[error("invalid digit at position {position}: '{digit}'")]
    InvalidDigit {
        /// 字符位置（1-indexed）
        position: usize,
        /// 非法字符
        digit: char,
    },
    /// 省份编码无效
    #[error("invalid province code: {0}")]
    InvalidProvince(String),
    /// 行业编码无效
    #[error("invalid industry code: {0}")]
    InvalidIndustry(String),
    /// 设备类型编码无效
    #[error("invalid device type code: {0}")]
    InvalidDeviceType(String),
    /// 部门编码超出范围
    #[error("invalid department code: {0}")]
    InvalidDepartment(String),
    /// 行业扩展编码超出范围
    #[error("invalid industry extension code: {0}")]
    InvalidIndustryExtension(String),
    /// 序号超出范围
    #[error("invalid serial number: {0}")]
    InvalidSerialNumber(String),
}
