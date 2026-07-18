//! GB28181 查询消息（Query）
//!
//! 对应 XML 根元素 `<Query>`，用于设备目录查询、设备信息查询、录像查询等。

use gb28181_codec::DeviceId;

use crate::types::{
    escape_xml, extract_tag_value, strip_xml_declaration, unescape_xml, CmdType, XmlError,
};

/// 查询消息
///
/// 对应 XML 格式：
/// ```xml
/// <?xml version="1.0" encoding="UTF-8"?>
/// <Query>
///   <CmdType>Catalog</CmdType>
///   <SN>1</SN>
///   <DeviceID>34020000002000000001</DeviceID>
/// </Query>
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// 命令类型
    pub cmd_type: CmdType,
    /// 命令序号
    pub sn: u32,
    /// 目标设备编码
    pub device_id: DeviceId,
    /// 录像查询开始时间（格式: 2024-01-01T00:00:00）
    pub start_time: Option<String>,
    /// 录像查询结束时间（格式: 2024-01-01T00:00:00）
    pub end_time: Option<String>,
}

impl Query {
    /// 创建设备目录查询
    pub fn catalog(sn: u32, device_id: DeviceId) -> Self {
        Self {
            cmd_type: CmdType::Catalog,
            sn,
            device_id,
            start_time: None,
            end_time: None,
        }
    }

    /// 创建设备信息查询
    pub fn device_info(sn: u32, device_id: DeviceId) -> Self {
        Self {
            cmd_type: CmdType::DeviceInfo,
            sn,
            device_id,
            start_time: None,
            end_time: None,
        }
    }

    /// 创建设备状态查询
    pub fn device_status(sn: u32, device_id: DeviceId) -> Self {
        Self {
            cmd_type: CmdType::DeviceStatus,
            sn,
            device_id,
            start_time: None,
            end_time: None,
        }
    }

    /// 创建录像查询
    ///
    /// `start_time` 和 `end_time` 格式为 `2024-01-01T00:00:00`。
    pub fn record_query(
        sn: u32,
        device_id: DeviceId,
        start_time: impl Into<String>,
        end_time: impl Into<String>,
    ) -> Self {
        Self {
            cmd_type: CmdType::RecordQuery,
            sn,
            device_id,
            start_time: Some(start_time.into()),
            end_time: Some(end_time.into()),
        }
    }

    /// 创建移动位置订阅
    pub fn mobile_position(sn: u32, device_id: DeviceId) -> Self {
        Self {
            cmd_type: CmdType::MobilePosition,
            sn,
            device_id,
            start_time: None,
            end_time: None,
        }
    }

    /// 创建配置下载查询
    pub fn config_download(sn: u32, device_id: DeviceId) -> Self {
        Self {
            cmd_type: CmdType::ConfigDownload,
            sn,
            device_id,
            start_time: None,
            end_time: None,
        }
    }

    /// 将查询消息序列化为 XML 字符串
    pub fn to_xml(&self) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Query>\n  <CmdType>{}</CmdType>\n  <SN>{}</SN>\n  <DeviceID>{}</DeviceID>",
            self.cmd_type.as_str(),
            self.sn,
            self.device_id,
        );

        if let Some(ref start_time) = self.start_time {
            xml.push_str(&format!(
                "\n  <StartTime>{}</StartTime>",
                escape_xml(start_time)
            ));
        }
        if let Some(ref end_time) = self.end_time {
            xml.push_str(&format!("\n  <EndTime>{}</EndTime>", escape_xml(end_time)));
        }

        xml.push_str("\n</Query>");
        xml
    }

    /// 从 XML 字符串解析查询消息
    pub fn from_xml(xml: &str) -> Result<Self, XmlError> {
        let body = strip_xml_declaration(xml);

        // 验证根元素
        let Some(inner) = extract_between_tags(body, "Query") else {
            return Err(XmlError::InvalidFormat(
                "missing <Query> root element".to_string(),
            ));
        };

        let cmd_type_str = extract_tag_value(&inner, "CmdType")
            .ok_or(XmlError::MissingField("CmdType".to_string()))?;
        let cmd_type = unescape_xml(&cmd_type_str).parse::<CmdType>().unwrap();

        let sn_str =
            extract_tag_value(&inner, "SN").ok_or(XmlError::MissingField("SN".to_string()))?;
        let sn: u32 = sn_str
            .parse()
            .map_err(|_| XmlError::InvalidNumber(format!("invalid SN: {sn_str}")))?;

        let device_id_str = extract_tag_value(&inner, "DeviceID")
            .ok_or(XmlError::MissingField("DeviceID".to_string()))?;
        let device_id =
            DeviceId::parse(&unescape_xml(&device_id_str)).map_err(XmlError::InvalidDeviceId)?;

        let start_time = extract_tag_value(&inner, "StartTime").map(|v| unescape_xml(&v));
        let end_time = extract_tag_value(&inner, "EndTime").map(|v| unescape_xml(&v));

        Ok(Self {
            cmd_type,
            sn,
            device_id,
            start_time,
            end_time,
        })
    }
}

/// 提取根标签之间的内容
fn extract_between_tags(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");

    let start = content.find(&open)?;
    let inner_start = start + open.len();
    let inner_end = content[inner_start..].find(&close)?;
    Some(content[inner_start..inner_start + inner_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_device_id(s: &str) -> DeviceId {
        DeviceId::parse(s).unwrap()
    }

    #[test]
    fn test_catalog_query_to_xml() {
        let device_id = make_device_id("34020000002000000001");
        let query = Query::catalog(1, device_id);
        let xml = query.to_xml();

        assert!(xml.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<Query>"));
        assert!(xml.contains("<CmdType>Catalog</CmdType>"));
        assert!(xml.contains("<SN>1</SN>"));
        assert!(xml.contains("<DeviceID>34020000002000000001</DeviceID>"));
        assert!(xml.contains("</Query>"));
        assert!(!xml.contains("<StartTime>"));
        assert!(!xml.contains("<EndTime>"));
    }

    #[test]
    fn test_device_info_query_to_xml() {
        let device_id = make_device_id("34020000001320000001");
        let query = Query::device_info(2, device_id);
        let xml = query.to_xml();

        assert!(xml.contains("<CmdType>DeviceInfo</CmdType>"));
        assert!(xml.contains("<SN>2</SN>"));
        assert!(xml.contains("<DeviceID>34020000001320000001</DeviceID>"));
    }

    #[test]
    fn test_catalog_query_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Query>
  <CmdType>Catalog</CmdType>
  <SN>1</SN>
  <DeviceID>34020000002000000001</DeviceID>
</Query>"#;

        let query = Query::from_xml(xml).unwrap();
        assert_eq!(query.cmd_type, CmdType::Catalog);
        assert_eq!(query.sn, 1);
        assert_eq!(query.device_id.as_str(), "34020000002000000001");
        assert!(query.start_time.is_none());
        assert!(query.end_time.is_none());
    }

    #[test]
    fn test_device_info_query_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Query>
  <CmdType>DeviceInfo</CmdType>
  <SN>2</SN>
  <DeviceID>34020000001320000001</DeviceID>
</Query>"#;

        let query = Query::from_xml(xml).unwrap();
        assert_eq!(query.cmd_type, CmdType::DeviceInfo);
        assert_eq!(query.sn, 2);
        assert_eq!(query.device_id.as_str(), "34020000001320000001");
    }

    #[test]
    fn test_record_query_to_xml() {
        let device_id = make_device_id("34020000001320000001");
        let query =
            Query::record_query(10, device_id, "2024-01-01T00:00:00", "2024-01-31T23:59:59");
        let xml = query.to_xml();

        assert!(xml.contains("<CmdType>RecordQuery</CmdType>"));
        assert!(xml.contains("<SN>10</SN>"));
        assert!(xml.contains("<DeviceID>34020000001320000001</DeviceID>"));
        assert!(xml.contains("<StartTime>2024-01-01T00:00:00</StartTime>"));
        assert!(xml.contains("<EndTime>2024-01-31T23:59:59</EndTime>"));
    }

    #[test]
    fn test_record_query_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Query>
  <CmdType>RecordQuery</CmdType>
  <SN>10</SN>
  <DeviceID>34020000001320000001</DeviceID>
  <StartTime>2024-01-01T00:00:00</StartTime>
  <EndTime>2024-01-31T23:59:59</EndTime>
</Query>"#;

        let query = Query::from_xml(xml).unwrap();
        assert_eq!(query.cmd_type, CmdType::RecordQuery);
        assert_eq!(query.sn, 10);
        assert_eq!(query.device_id.as_str(), "34020000001320000001");
        assert_eq!(query.start_time.as_deref(), Some("2024-01-01T00:00:00"));
        assert_eq!(query.end_time.as_deref(), Some("2024-01-31T23:59:59"));
    }

    #[test]
    fn test_device_status_query() {
        let device_id = make_device_id("34020000001320000001");
        let query = Query::device_status(5, device_id);
        let xml = query.to_xml();
        let parsed = Query::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::DeviceStatus);
        assert_eq!(parsed.sn, 5);
        assert!(parsed.start_time.is_none());
        assert!(parsed.end_time.is_none());
    }

    #[test]
    fn test_mobile_position_query() {
        let device_id = make_device_id("34020000001320000001");
        let query = Query::mobile_position(7, device_id);
        let xml = query.to_xml();
        let parsed = Query::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::MobilePosition);
        assert_eq!(parsed.sn, 7);
    }

    #[test]
    fn test_query_roundtrip() {
        let device_id = make_device_id("34020000002000000001");
        let original = Query::catalog(42, device_id);
        let xml = original.to_xml();
        let parsed = Query::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, original.cmd_type);
        assert_eq!(parsed.sn, original.sn);
        assert_eq!(parsed.device_id, original.device_id);
    }

    #[test]
    fn test_query_roundtrip_device_info() {
        let device_id = make_device_id("34020000001320000001");
        let original = Query::device_info(99, device_id);
        let xml = original.to_xml();
        let parsed = Query::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, original.cmd_type);
        assert_eq!(parsed.sn, original.sn);
        assert_eq!(parsed.device_id, original.device_id);
    }

    #[test]
    fn test_record_query_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let original =
            Query::record_query(10, device_id, "2024-01-01T00:00:00", "2024-01-31T23:59:59");
        let xml = original.to_xml();
        let parsed = Query::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, original.cmd_type);
        assert_eq!(parsed.sn, original.sn);
        assert_eq!(parsed.device_id, original.device_id);
        assert_eq!(parsed.start_time, original.start_time);
        assert_eq!(parsed.end_time, original.end_time);
    }

    #[test]
    fn test_query_from_xml_missing_field() {
        let xml = "<Query><CmdType>Catalog</CmdType></Query>";
        let result = Query::from_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_from_xml_invalid_sn() {
        let xml = "<Query><CmdType>Catalog</CmdType><SN>abc</SN><DeviceID>34020000002000000001</DeviceID></Query>";
        let result = Query::from_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_from_xml_no_root() {
        let xml = "<Other><CmdType>Catalog</CmdType></Other>";
        let result = Query::from_xml(xml);
        assert!(result.is_err());
    }
}
