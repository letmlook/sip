//! GB28181 XML 公共类型定义
//!
//! 包含命令类型枚举、设备信息结构体和错误类型。

use gb28181_codec::DeviceId;
use std::fmt;
use std::str::FromStr;

/// GB28181 命令类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdType {
    /// 设备目录查询/响应
    Catalog,
    /// 设备信息查询/响应
    DeviceInfo,
    /// 设备控制
    DeviceControl,
    /// 心跳保活
    Keepalive,
    /// 配置下载
    ConfigDownload,
    /// 配置上传
    ConfigUpload,
    /// 报警通知
    Alarm,
    /// 报警命令
    AlarmCmd,
    /// 录像查询
    RecordQuery,
    /// 设备状态查询
    DeviceStatus,
    /// 移动位置订阅
    MobilePosition,
    /// 移动位置通知
    MobilePositionNotify,
    /// 语音广播
    Broadcast,
    /// 其他未知命令类型
    Other(String),
}

impl CmdType {
    /// 获取命令类型的字符串标识
    pub fn as_str(&self) -> &str {
        match self {
            CmdType::Catalog => "Catalog",
            CmdType::DeviceInfo => "DeviceInfo",
            CmdType::DeviceControl => "DeviceControl",
            CmdType::Keepalive => "Keepalive",
            CmdType::ConfigDownload => "ConfigDownload",
            CmdType::ConfigUpload => "ConfigUpload",
            CmdType::Alarm => "Alarm",
            CmdType::AlarmCmd => "AlarmCmd",
            CmdType::RecordQuery => "RecordQuery",
            CmdType::DeviceStatus => "DeviceStatus",
            CmdType::MobilePosition => "MobilePosition",
            CmdType::MobilePositionNotify => "MobilePositionNotify",
            CmdType::Broadcast => "Broadcast",
            CmdType::Other(s) => s.as_str(),
        }
    }
}

impl FromStr for CmdType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Catalog" => CmdType::Catalog,
            "DeviceInfo" => CmdType::DeviceInfo,
            "DeviceControl" => CmdType::DeviceControl,
            "Keepalive" => CmdType::Keepalive,
            "ConfigDownload" => CmdType::ConfigDownload,
            "ConfigUpload" => CmdType::ConfigUpload,
            "Alarm" => CmdType::Alarm,
            "AlarmCmd" => CmdType::AlarmCmd,
            "RecordQuery" => CmdType::RecordQuery,
            "DeviceStatus" => CmdType::DeviceStatus,
            "MobilePosition" => CmdType::MobilePosition,
            "MobilePositionNotify" => CmdType::MobilePositionNotify,
            "Broadcast" => CmdType::Broadcast,
            other => CmdType::Other(other.to_string()),
        })
    }
}

impl fmt::Display for CmdType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 设备目录项
///
/// 对应 Catalog 响应中 DeviceList/Item 的内容。
/// 大部分字段为可选，因为实际设备可能只返回部分信息。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceItem {
    /// 设备编码（必填）
    pub device_id: DeviceId,
    /// 设备名称
    pub name: Option<String>,
    /// 厂商
    pub manufacturer: Option<String>,
    /// 型号
    pub model: Option<String>,
    /// 所属者
    pub owner: Option<String>,
    /// 行政区划
    pub civil_code: Option<String>,
    /// 警区
    pub block: Option<String>,
    /// 安装地址
    pub address: Option<String>,
    /// 是否有子设备（0=无, 1=有）
    pub parental: Option<u32>,
    /// 父设备编码
    pub parent_id: Option<String>,
    /// 安全传输方式
    pub safety_way: Option<u32>,
    /// 注册方式（1=符合GB/T 28181, 2=其他）
    pub register_way: Option<u32>,
    /// 证书编号
    pub cert_num: Option<String>,
    /// 是否可认证（0=未认证, 1=已认证）
    pub certifiable: Option<u32>,
    /// 错误码
    pub err_code: Option<u32>,
    /// 结束时间
    pub end_time: Option<String>,
    /// 保密属性（0=不涉密, 1=涉密）
    pub secrecy: Option<u32>,
    /// IP 地址
    pub ip_address: Option<String>,
    /// 端口
    pub port: Option<u32>,
    /// 密码
    pub password: Option<String>,
    /// 云台类型（0=未知, 1=球机, 2=半球, 3=固定枪机, 4=遥控枪机）
    pub ptz_type: Option<u32>,
    /// 设备状态（ON=在线, OFF=离线）
    pub status: Option<String>,
    /// 经度
    pub longitude: Option<f64>,
    /// 纬度
    pub latitude: Option<f64>,
}

impl DeviceItem {
    /// 创建一个只包含设备编码的 DeviceItem
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            name: None,
            manufacturer: None,
            model: None,
            owner: None,
            civil_code: None,
            block: None,
            address: None,
            parental: None,
            parent_id: None,
            safety_way: None,
            register_way: None,
            cert_num: None,
            certifiable: None,
            err_code: None,
            end_time: None,
            secrecy: None,
            ip_address: None,
            port: None,
            password: None,
            ptz_type: None,
            status: None,
            longitude: None,
            latitude: None,
        }
    }

    /// 将 DeviceItem 序列化为 XML Item 元素内容
    pub fn to_xml_item(&self) -> String {
        let mut xml = String::from("    <Item>\n");
        xml.push_str(&format!("      <DeviceID>{}</DeviceID>\n", self.device_id));
        if let Some(ref v) = self.name {
            xml.push_str(&format!("      <Name>{}</Name>\n", escape_xml(v)));
        }
        if let Some(ref v) = self.manufacturer {
            xml.push_str(&format!(
                "      <Manufacturer>{}</Manufacturer>\n",
                escape_xml(v)
            ));
        }
        if let Some(ref v) = self.model {
            xml.push_str(&format!("      <Model>{}</Model>\n", escape_xml(v)));
        }
        if let Some(ref v) = self.owner {
            xml.push_str(&format!("      <Owner>{}</Owner>\n", escape_xml(v)));
        }
        if let Some(ref v) = self.civil_code {
            xml.push_str(&format!("      <CivilCode>{}</CivilCode>\n", escape_xml(v)));
        }
        if let Some(ref v) = self.block {
            xml.push_str(&format!("      <Block>{}</Block>\n", escape_xml(v)));
        }
        if let Some(ref v) = self.address {
            xml.push_str(&format!("      <Address>{}</Address>\n", escape_xml(v)));
        }
        if let Some(v) = self.parental {
            xml.push_str(&format!("      <Parental>{}</Parental>\n", v));
        }
        if let Some(ref v) = self.parent_id {
            xml.push_str(&format!("      <ParentID>{}</ParentID>\n", escape_xml(v)));
        }
        if let Some(v) = self.safety_way {
            xml.push_str(&format!("      <SafetyWay>{}</SafetyWay>\n", v));
        }
        if let Some(v) = self.register_way {
            xml.push_str(&format!("      <RegisterWay>{}</RegisterWay>\n", v));
        }
        if let Some(ref v) = self.cert_num {
            xml.push_str(&format!("      <CertNum>{}</CertNum>\n", escape_xml(v)));
        }
        if let Some(v) = self.certifiable {
            xml.push_str(&format!("      <Certifiable>{}</Certifiable>\n", v));
        }
        if let Some(v) = self.err_code {
            xml.push_str(&format!("      <ErrCode>{}</ErrCode>\n", v));
        }
        if let Some(ref v) = self.end_time {
            xml.push_str(&format!("      <EndTime>{}</EndTime>\n", escape_xml(v)));
        }
        if let Some(v) = self.secrecy {
            xml.push_str(&format!("      <Secrecy>{}</Secrecy>\n", v));
        }
        if let Some(ref v) = self.ip_address {
            xml.push_str(&format!("      <IPAddress>{}</IPAddress>\n", escape_xml(v)));
        }
        if let Some(v) = self.port {
            xml.push_str(&format!("      <Port>{}</Port>\n", v));
        }
        if let Some(ref v) = self.password {
            xml.push_str(&format!("      <Password>{}</Password>\n", escape_xml(v)));
        }
        if let Some(v) = self.ptz_type {
            xml.push_str(&format!("      <PTZType>{}</PTZType>\n", v));
        }
        if let Some(ref v) = self.status {
            xml.push_str(&format!("      <Status>{}</Status>\n", escape_xml(v)));
        }
        if let Some(v) = self.longitude {
            xml.push_str(&format!("      <Longitude>{}</Longitude>\n", v));
        }
        if let Some(v) = self.latitude {
            xml.push_str(&format!("      <Latitude>{}</Latitude>\n", v));
        }
        xml.push_str("    </Item>\n");
        xml
    }

    /// 从 XML Item 元素内容解析 DeviceItem
    ///
    /// `content` 应为 `<Item>...</Item>` 标签之间的内容（不含 Item 标签本身）
    pub fn from_xml_item(content: &str) -> Result<Self, XmlError> {
        let device_id_str = extract_tag_value(content, "DeviceID")
            .ok_or(XmlError::MissingField("DeviceID".to_string()))?;
        let device_id =
            DeviceId::parse(&unescape_xml(&device_id_str)).map_err(XmlError::InvalidDeviceId)?;

        let mut item = DeviceItem::new(device_id);
        item.name = extract_tag_value(content, "Name");
        item.manufacturer = extract_tag_value(content, "Manufacturer");
        item.model = extract_tag_value(content, "Model");
        item.owner = extract_tag_value(content, "Owner");
        item.civil_code = extract_tag_value(content, "CivilCode");
        item.block = extract_tag_value(content, "Block");
        item.address = extract_tag_value(content, "Address");
        item.parental = extract_tag_value(content, "Parental").and_then(|v| v.parse().ok());
        item.parent_id = extract_tag_value(content, "ParentID");
        item.safety_way = extract_tag_value(content, "SafetyWay").and_then(|v| v.parse().ok());
        item.register_way = extract_tag_value(content, "RegisterWay").and_then(|v| v.parse().ok());
        item.cert_num = extract_tag_value(content, "CertNum");
        item.certifiable = extract_tag_value(content, "Certifiable").and_then(|v| v.parse().ok());
        item.err_code = extract_tag_value(content, "ErrCode").and_then(|v| v.parse().ok());
        item.end_time = extract_tag_value(content, "EndTime");
        item.secrecy = extract_tag_value(content, "Secrecy").and_then(|v| v.parse().ok());
        item.ip_address = extract_tag_value(content, "IPAddress");
        item.port = extract_tag_value(content, "Port").and_then(|v| v.parse().ok());
        item.password = extract_tag_value(content, "Password");
        item.ptz_type = extract_tag_value(content, "PTZType").and_then(|v| v.parse().ok());
        item.status = extract_tag_value(content, "Status");
        item.longitude = extract_tag_value(content, "Longitude").and_then(|v| v.parse().ok());
        item.latitude = extract_tag_value(content, "Latitude").and_then(|v| v.parse().ok());

        Ok(item)
    }
}

/// 录像项
///
/// 对应 RecordInfo 响应中 RecordList/Item 的内容。
#[derive(Debug, Clone, PartialEq)]
pub struct RecordItem {
    /// 设备编码
    pub device_id: String,
    /// 录像名称
    pub name: String,
    /// 录像开始时间（格式: 2024-01-01T00:00:00）
    pub start_time: String,
    /// 录像结束时间（格式: 2024-01-01T00:00:00）
    pub end_time: String,
}

impl RecordItem {
    /// 创建录像项
    pub fn new(
        device_id: impl Into<String>,
        name: impl Into<String>,
        start_time: impl Into<String>,
        end_time: impl Into<String>,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            name: name.into(),
            start_time: start_time.into(),
            end_time: end_time.into(),
        }
    }

    /// 将 RecordItem 序列化为 XML Item 元素内容
    pub fn to_xml_item(&self) -> String {
        let mut xml = String::from("    <Item>\n");
        xml.push_str(&format!(
            "      <DeviceID>{}</DeviceID>\n",
            escape_xml(&self.device_id)
        ));
        xml.push_str(&format!("      <Name>{}</Name>\n", escape_xml(&self.name)));
        xml.push_str(&format!(
            "      <StartTime>{}</StartTime>\n",
            escape_xml(&self.start_time)
        ));
        xml.push_str(&format!(
            "      <EndTime>{}</EndTime>\n",
            escape_xml(&self.end_time)
        ));
        xml.push_str("    </Item>\n");
        xml
    }

    /// 从 XML Item 元素内容解析 RecordItem
    pub fn from_xml_item(content: &str) -> Result<Self, XmlError> {
        let device_id = extract_tag_value(content, "DeviceID")
            .ok_or(XmlError::MissingField("DeviceID".to_string()))?;
        let name =
            extract_tag_value(content, "Name").ok_or(XmlError::MissingField("Name".to_string()))?;
        let start_time = extract_tag_value(content, "StartTime")
            .ok_or(XmlError::MissingField("StartTime".to_string()))?;
        let end_time = extract_tag_value(content, "EndTime")
            .ok_or(XmlError::MissingField("EndTime".to_string()))?;

        Ok(Self {
            device_id: unescape_xml(&device_id),
            name: unescape_xml(&name),
            start_time: unescape_xml(&start_time),
            end_time: unescape_xml(&end_time),
        })
    }
}

/// 报警信息
///
/// 对应 Alarm 通知/响应中 AlarmList/Item 的内容。
#[derive(Debug, Clone, PartialEq)]
pub struct AlarmInfo {
    /// 报警设备编码
    pub device_id: String,
    /// 报警方式
    pub alarm_method: String,
    /// 报警时间（格式: 2024-01-01T00:00:00）
    pub alarm_time: String,
    /// 报警描述
    pub alarm_description: Option<String>,
}

impl AlarmInfo {
    /// 创建报警信息
    pub fn new(
        device_id: impl Into<String>,
        alarm_method: impl Into<String>,
        alarm_time: impl Into<String>,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            alarm_method: alarm_method.into(),
            alarm_time: alarm_time.into(),
            alarm_description: None,
        }
    }

    /// 将 AlarmInfo 序列化为 XML Item 元素内容
    pub fn to_xml_item(&self) -> String {
        let mut xml = String::from("    <Item>\n");
        xml.push_str(&format!(
            "      <DeviceID>{}</DeviceID>\n",
            escape_xml(&self.device_id)
        ));
        xml.push_str(&format!(
            "      <AlarmMethod>{}</AlarmMethod>\n",
            escape_xml(&self.alarm_method)
        ));
        xml.push_str(&format!(
            "      <AlarmTime>{}</AlarmTime>\n",
            escape_xml(&self.alarm_time)
        ));
        if let Some(ref desc) = self.alarm_description {
            xml.push_str(&format!(
                "      <AlarmDescription>{}</AlarmDescription>\n",
                escape_xml(desc)
            ));
        }
        xml.push_str("    </Item>\n");
        xml
    }

    /// 从 XML Item 元素内容解析 AlarmInfo
    pub fn from_xml_item(content: &str) -> Result<Self, XmlError> {
        let device_id = extract_tag_value(content, "DeviceID")
            .ok_or(XmlError::MissingField("DeviceID".to_string()))?;
        let alarm_method = extract_tag_value(content, "AlarmMethod")
            .ok_or(XmlError::MissingField("AlarmMethod".to_string()))?;
        let alarm_time = extract_tag_value(content, "AlarmTime")
            .ok_or(XmlError::MissingField("AlarmTime".to_string()))?;
        let alarm_description = extract_tag_value(content, "AlarmDescription");

        Ok(Self {
            device_id: unescape_xml(&device_id),
            alarm_method: unescape_xml(&alarm_method),
            alarm_time: unescape_xml(&alarm_time),
            alarm_description: alarm_description.map(|v| unescape_xml(&v)),
        })
    }
}

/// 移动位置信息
///
/// 对应 MobilePosition 通知中 MobilePositionList/Item 的内容。
#[derive(Debug, Clone, PartialEq)]
pub struct MobilePositionInfo {
    /// 设备编码
    pub device_id: String,
    /// 经度
    pub longitude: f64,
    /// 纬度
    pub latitude: f64,
    /// 海拔
    pub altitude: Option<f64>,
    /// 速度
    pub speed: Option<f64>,
    /// 方向
    pub direction: Option<f64>,
    /// 上报时间（格式: 2024-01-01T00:00:00）
    pub report_time: String,
}

impl MobilePositionInfo {
    /// 创建移动位置信息
    pub fn new(
        device_id: impl Into<String>,
        longitude: f64,
        latitude: f64,
        report_time: impl Into<String>,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            longitude,
            latitude,
            altitude: None,
            speed: None,
            direction: None,
            report_time: report_time.into(),
        }
    }

    /// 将 MobilePositionInfo 序列化为 XML Item 元素内容
    pub fn to_xml_item(&self) -> String {
        let mut xml = String::from("    <Item>\n");
        xml.push_str(&format!(
            "      <DeviceID>{}</DeviceID>\n",
            escape_xml(&self.device_id)
        ));
        xml.push_str(&format!(
            "      <Longitude>{}</Longitude>\n",
            self.longitude
        ));
        xml.push_str(&format!("      <Latitude>{}</Latitude>\n", self.latitude));
        if let Some(v) = self.altitude {
            xml.push_str(&format!("      <Altitude>{}</Altitude>\n", v));
        }
        if let Some(v) = self.speed {
            xml.push_str(&format!("      <Speed>{}</Speed>\n", v));
        }
        if let Some(v) = self.direction {
            xml.push_str(&format!("      <Direction>{}</Direction>\n", v));
        }
        xml.push_str(&format!(
            "      <ReportTime>{}</ReportTime>\n",
            escape_xml(&self.report_time)
        ));
        xml.push_str("    </Item>\n");
        xml
    }

    /// 从 XML Item 元素内容解析 MobilePositionInfo
    pub fn from_xml_item(content: &str) -> Result<Self, XmlError> {
        let device_id = extract_tag_value(content, "DeviceID")
            .ok_or(XmlError::MissingField("DeviceID".to_string()))?;
        let longitude_str = extract_tag_value(content, "Longitude")
            .ok_or(XmlError::MissingField("Longitude".to_string()))?;
        let longitude: f64 = longitude_str
            .parse()
            .map_err(|_| XmlError::InvalidNumber(format!("invalid Longitude: {longitude_str}")))?;
        let latitude_str = extract_tag_value(content, "Latitude")
            .ok_or(XmlError::MissingField("Latitude".to_string()))?;
        let latitude: f64 = latitude_str
            .parse()
            .map_err(|_| XmlError::InvalidNumber(format!("invalid Latitude: {latitude_str}")))?;
        let altitude = extract_tag_value(content, "Altitude").and_then(|v| v.parse().ok());
        let speed = extract_tag_value(content, "Speed").and_then(|v| v.parse().ok());
        let direction = extract_tag_value(content, "Direction").and_then(|v| v.parse().ok());
        let report_time = extract_tag_value(content, "ReportTime")
            .ok_or(XmlError::MissingField("ReportTime".to_string()))?;

        Ok(Self {
            device_id: unescape_xml(&device_id),
            longitude,
            latitude,
            altitude,
            speed,
            direction,
            report_time: unescape_xml(&report_time),
        })
    }
}

/// 设备状态信息
///
/// 对应 DeviceStatus 响应中的状态字段。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceStatusInfo {
    /// 是否在线
    pub online: bool,
    /// 设备状态描述
    pub status: String,
    /// 是否编码中
    pub encode: Option<bool>,
    /// 是否录像中
    pub record: Option<bool>,
    /// 设备时间（格式: 2024-01-01T00:00:00）
    pub device_time: Option<String>,
    /// 报警状态
    pub alarmstatus: Option<String>,
}

impl DeviceStatusInfo {
    /// 创建设备状态信息
    pub fn new(online: bool, status: impl Into<String>) -> Self {
        Self {
            online,
            status: status.into(),
            encode: None,
            record: None,
            device_time: None,
            alarmstatus: None,
        }
    }

    /// 将 DeviceStatusInfo 序列化为 XML 元素
    pub fn to_xml_elements(&self) -> String {
        let mut xml = String::new();
        xml.push_str("  <Result>OK</Result>\n");
        xml.push_str(&format!(
            "  <Online>{}</Online>\n",
            if self.online { "ONLINE" } else { "OFFLINE" }
        ));
        xml.push_str(&format!(
            "  <Status>{}</Status>\n",
            escape_xml(&self.status)
        ));
        if let Some(v) = self.encode {
            xml.push_str(&format!(
                "  <Encode>{}</Encode>\n",
                if v { "ON" } else { "OFF" }
            ));
        }
        if let Some(v) = self.record {
            xml.push_str(&format!(
                "  <Record>{}</Record>\n",
                if v { "ON" } else { "OFF" }
            ));
        }
        if let Some(ref v) = self.device_time {
            xml.push_str(&format!("  <DeviceTime>{}</DeviceTime>\n", escape_xml(v)));
        }
        if let Some(ref v) = self.alarmstatus {
            xml.push_str(&format!("  <Alarmstatus>{}</Alarmstatus>\n", escape_xml(v)));
        }
        xml
    }

    /// 从 XML 内容解析 DeviceStatusInfo
    pub fn from_xml_elements(content: &str) -> Result<Self, XmlError> {
        let online_str = extract_tag_value(content, "Online")
            .ok_or(XmlError::MissingField("Online".to_string()))?;
        let online = online_str.to_uppercase() == "ONLINE";
        let status = extract_tag_value(content, "Status")
            .ok_or(XmlError::MissingField("Status".to_string()))?;
        let encode = extract_tag_value(content, "Encode").map(|v| v.to_uppercase() == "ON");
        let record = extract_tag_value(content, "Record").map(|v| v.to_uppercase() == "ON");
        let device_time = extract_tag_value(content, "DeviceTime");
        let alarmstatus = extract_tag_value(content, "Alarmstatus");

        Ok(Self {
            online,
            status: unescape_xml(&status),
            encode,
            record,
            device_time: device_time.map(|v| unescape_xml(&v)),
            alarmstatus: alarmstatus.map(|v| unescape_xml(&v)),
        })
    }
}

/// XML 解析/构建错误
#[derive(Debug, thiserror::Error)]
pub enum XmlError {
    /// 缺少必填字段
    #[error("missing required field: {0}")]
    MissingField(String),
    /// 无效的 XML 格式
    #[error("invalid XML format: {0}")]
    InvalidFormat(String),
    /// 未知根元素
    #[error("unknown root element: {0}")]
    UnknownRoot(String),
    /// 数值解析错误
    #[error("invalid number: {0}")]
    InvalidNumber(String),
    /// 设备编码错误
    #[error("invalid device ID: {0}")]
    InvalidDeviceId(#[from] gb28181_codec::CodecError),
}

// ===== XML 辅助函数 =====

/// XML 声明头
pub(crate) const XML_DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>"#;

/// 从 XML 文本中提取指定标签的值
///
/// 查找 `<tag>value</tag>` 模式，返回 value 部分。
/// 如果标签不存在，返回 None。
/// 如果标签存在但为空（如 `<Tag></Tag>`），返回 Some("")。
pub(crate) fn extract_tag_value(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");

    let start = content.find(&open)?;
    let value_start = start + open.len();
    let value_end = content[value_start..].find(&close)?;
    Some(content[value_start..value_start + value_end].to_string())
}

/// 提取包含属性和子元素的标签内容
///
/// 查找 `<tag ...>...</tag>` 模式，返回 (属性字符串, 内容)。
/// 例如 `<DeviceList Num="2">...</DeviceList>` 返回 (`Num="2"`, `...`)
pub(crate) fn extract_tag_with_attrs(content: &str, tag: &str) -> Option<(String, String)> {
    let open_start = format!("<{tag}");
    let close = format!("</{tag}>");

    let start = content.find(&open_start)?;
    // 找到 > 结束开始标签
    let tag_content_start = start + open_start.len();
    let gt_pos = content[tag_content_start..].find('>')?;
    let attrs = content[tag_content_start..tag_content_start + gt_pos]
        .trim()
        .to_string();
    let inner_start = tag_content_start + gt_pos + 1;
    let inner_end = content[inner_start..].find(&close)?;
    let inner = content[inner_start..inner_start + inner_end].to_string();

    Some((attrs, inner))
}

/// 从属性字符串中提取指定属性的值
///
/// 例如从 `Num="2"` 中提取 `Num` 属性，返回 `2`。
#[allow(dead_code)]
pub(crate) fn extract_attr(attrs: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("{attr_name}=\"");
    let start = attrs.find(&pattern)?;
    let value_start = start + pattern.len();
    let value_end = attrs[value_start..].find('"')?;
    Some(attrs[value_start..value_start + value_end].to_string())
}

/// 提取 XML 中所有同名标签的内容
///
/// 查找所有 `<tag>...</tag>` 或 `<tag ...>...</tag>` 模式，
/// 返回每个匹配的内部内容。
pub(crate) fn extract_all_tags(content: &str, tag: &str) -> Vec<String> {
    let mut results = Vec::new();
    let open_start = format!("<{tag}");
    let close = format!("</{tag}>");

    let mut search_from = 0;
    while search_from < content.len() {
        let Some(start) = content[search_from..].find(&open_start) else {
            break;
        };
        let abs_start = search_from + start;
        let tag_content_start = abs_start + open_start.len();

        let Some(gt_offset) = content[tag_content_start..].find('>') else {
            break;
        };
        let inner_start = tag_content_start + gt_offset + 1;

        let Some(inner_end) = content[inner_start..].find(&close) else {
            break;
        };

        results.push(content[inner_start..inner_start + inner_end].to_string());
        search_from = inner_start + inner_end + close.len();
    }

    results
}

/// 转义 XML 特殊字符
pub(crate) fn escape_xml(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(c),
        }
    }
    result
}

/// 反转义 XML 特殊字符
pub(crate) fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// 去除 XML 声明头，返回正文内容
pub(crate) fn strip_xml_declaration(content: &str) -> &str {
    content
        .trim()
        .strip_prefix(XML_DECLARATION)
        .map(|s| s.trim())
        .unwrap_or(content.trim())
}

/// 判断根元素类型
///
/// 返回根元素名称（如 "Query"、"Response"、"Control"）
pub(crate) fn detect_root_element(content: &str) -> Result<&str, XmlError> {
    let body = strip_xml_declaration(content);
    // 查找第一个 < 后面的元素名
    if let Some(start) = body.find('<') {
        let rest = &body[start + 1..];
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(rest.len());
        if name_end == 0 {
            return Err(XmlError::InvalidFormat("empty root element".to_string()));
        }
        Ok(&rest[..name_end])
    } else {
        Err(XmlError::InvalidFormat("no XML element found".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmd_type_roundtrip() {
        let types = [
            CmdType::Catalog,
            CmdType::DeviceInfo,
            CmdType::DeviceControl,
            CmdType::Keepalive,
            CmdType::ConfigDownload,
            CmdType::ConfigUpload,
            CmdType::Alarm,
            CmdType::AlarmCmd,
            CmdType::RecordQuery,
            CmdType::DeviceStatus,
            CmdType::MobilePosition,
            CmdType::MobilePositionNotify,
            CmdType::Broadcast,
        ];
        for t in types {
            assert_eq!(t.as_str().parse::<CmdType>().unwrap(), t);
        }
    }

    #[test]
    fn test_cmd_type_other() {
        let t = "UnknownCmd".parse::<CmdType>().unwrap();
        assert_eq!(t, CmdType::Other("UnknownCmd".to_string()));
        assert_eq!(t.as_str(), "UnknownCmd");
    }

    #[test]
    fn test_extract_tag_value() {
        let xml = "<CmdType>Catalog</CmdType><SN>1</SN>";
        assert_eq!(
            extract_tag_value(xml, "CmdType"),
            Some("Catalog".to_string())
        );
        assert_eq!(extract_tag_value(xml, "SN"), Some("1".to_string()));
        assert_eq!(extract_tag_value(xml, "Missing"), None);
    }

    #[test]
    fn test_extract_tag_empty_value() {
        let xml = "<CertNum></CertNum><Name>test</Name>";
        assert_eq!(extract_tag_value(xml, "CertNum"), Some("".to_string()));
        assert_eq!(extract_tag_value(xml, "Name"), Some("test".to_string()));
    }

    #[test]
    fn test_extract_tag_with_attrs() {
        let xml = r#"<DeviceList Num="2"><Item>inner</Item></DeviceList>"#;
        let (attrs, inner) = extract_tag_with_attrs(xml, "DeviceList").unwrap();
        assert_eq!(attrs, r#"Num="2""#);
        assert!(inner.contains("<Item>inner</Item>"));
    }

    #[test]
    fn test_extract_attr() {
        let attrs = r#"Num="2""#;
        assert_eq!(extract_attr(attrs, "Num"), Some("2".to_string()));
        assert_eq!(extract_attr(attrs, "Missing"), None);
    }

    #[test]
    fn test_extract_all_tags() {
        let xml = "<Item><DeviceID>1</DeviceID></Item><Item><DeviceID>2</DeviceID></Item>";
        let items = extract_all_tags(xml, "Item");
        assert_eq!(items.len(), 2);
        assert!(items[0].contains("<DeviceID>1</DeviceID>"));
        assert!(items[1].contains("<DeviceID>2</DeviceID>"));
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(
            escape_xml("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
    }

    #[test]
    fn test_unescape_xml() {
        assert_eq!(
            unescape_xml("a&amp;b&lt;c&gt;d&quot;e&apos;f"),
            "a&b<c>d\"e'f"
        );
    }

    #[test]
    fn test_detect_root_element() {
        assert_eq!(
            detect_root_element("<Query><CmdType>Catalog</CmdType></Query>").unwrap(),
            "Query"
        );
        assert_eq!(
            detect_root_element(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Response></Response>"
            )
            .unwrap(),
            "Response"
        );
        assert_eq!(
            detect_root_element("<Control></Control>").unwrap(),
            "Control"
        );
    }

    #[test]
    fn test_device_item_roundtrip() {
        let id = DeviceId::parse("34020000001320000001").unwrap();
        let mut item = DeviceItem::new(id);
        item.name = Some("Camera1".to_string());
        item.manufacturer = Some("Hikvision".to_string());
        item.status = Some("ON".to_string());

        let xml = item.to_xml_item();
        let parsed = DeviceItem::from_xml_item(&xml).unwrap();
        assert_eq!(parsed.device_id, item.device_id);
        assert_eq!(parsed.name, item.name);
        assert_eq!(parsed.manufacturer, item.manufacturer);
        assert_eq!(parsed.status, item.status);
    }

    #[test]
    fn test_record_item_roundtrip() {
        let item = RecordItem::new(
            "34020000001320000001",
            "录像1",
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
        );
        let xml = item.to_xml_item();
        let parsed = RecordItem::from_xml_item(&xml).unwrap();
        assert_eq!(parsed.device_id, item.device_id);
        assert_eq!(parsed.name, item.name);
        assert_eq!(parsed.start_time, item.start_time);
        assert_eq!(parsed.end_time, item.end_time);
    }

    #[test]
    fn test_alarm_info_roundtrip() {
        let mut info = AlarmInfo::new("34020000001320000001", "1", "2024-01-01T12:00:00");
        info.alarm_description = Some("运动检测报警".to_string());
        let xml = info.to_xml_item();
        let parsed = AlarmInfo::from_xml_item(&xml).unwrap();
        assert_eq!(parsed.device_id, info.device_id);
        assert_eq!(parsed.alarm_method, info.alarm_method);
        assert_eq!(parsed.alarm_time, info.alarm_time);
        assert_eq!(parsed.alarm_description, info.alarm_description);
    }

    #[test]
    fn test_alarm_info_no_description() {
        let info = AlarmInfo::new("34020000001320000001", "2", "2024-06-15T08:30:00");
        let xml = info.to_xml_item();
        let parsed = AlarmInfo::from_xml_item(&xml).unwrap();
        assert_eq!(parsed.device_id, info.device_id);
        assert!(parsed.alarm_description.is_none());
    }

    #[test]
    fn test_mobile_position_info_roundtrip() {
        let mut info = MobilePositionInfo::new(
            "34020000001320000001",
            116.397,
            39.908,
            "2024-01-01T12:00:00",
        );
        info.altitude = Some(50.0);
        info.speed = Some(60.5);
        info.direction = Some(180.0);
        let xml = info.to_xml_item();
        let parsed = MobilePositionInfo::from_xml_item(&xml).unwrap();
        assert_eq!(parsed.device_id, info.device_id);
        assert!((parsed.longitude - info.longitude).abs() < f64::EPSILON);
        assert!((parsed.latitude - info.latitude).abs() < f64::EPSILON);
        assert_eq!(parsed.altitude, info.altitude);
        assert_eq!(parsed.speed, info.speed);
        assert_eq!(parsed.direction, info.direction);
        assert_eq!(parsed.report_time, info.report_time);
    }

    #[test]
    fn test_mobile_position_info_minimal() {
        let info = MobilePositionInfo::new(
            "34020000001320000001",
            116.397,
            39.908,
            "2024-01-01T12:00:00",
        );
        let xml = info.to_xml_item();
        let parsed = MobilePositionInfo::from_xml_item(&xml).unwrap();
        assert!(parsed.altitude.is_none());
        assert!(parsed.speed.is_none());
        assert!(parsed.direction.is_none());
    }

    #[test]
    fn test_device_status_info_roundtrip() {
        let mut info = DeviceStatusInfo::new(true, "OK".to_string());
        info.encode = Some(true);
        info.record = Some(false);
        info.device_time = Some("2024-01-01T12:00:00".to_string());
        info.alarmstatus = Some("Alarm".to_string());
        let xml = info.to_xml_elements();
        let parsed = DeviceStatusInfo::from_xml_elements(&xml).unwrap();
        assert_eq!(parsed.online, info.online);
        assert_eq!(parsed.status, info.status);
        assert_eq!(parsed.encode, info.encode);
        assert_eq!(parsed.record, info.record);
        assert_eq!(parsed.device_time, info.device_time);
        assert_eq!(parsed.alarmstatus, info.alarmstatus);
    }

    #[test]
    fn test_device_status_info_offline() {
        let info = DeviceStatusInfo::new(false, "OFFLINE".to_string());
        let xml = info.to_xml_elements();
        assert!(xml.contains("<Online>OFFLINE</Online>"));
        let parsed = DeviceStatusInfo::from_xml_elements(&xml).unwrap();
        assert!(!parsed.online);
        assert!(parsed.encode.is_none());
        assert!(parsed.record.is_none());
    }
}
