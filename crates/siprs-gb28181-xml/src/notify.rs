//! GB28181 通知消息（Notify）
//!
//! 对应 XML 根元素 `<Notify>`，用于心跳保活、报警通知、移动位置通知等。
//! 与 `<Response>` 不同，`<Notify>` 是设备端主动上报的消息。

use siprs_gb28181_codec::DeviceId;

use crate::types::{
    escape_xml, extract_all_tags, extract_tag_value, extract_tag_with_attrs, strip_xml_declaration,
    unescape_xml, AlarmInfo, CmdType, MobilePositionInfo, XmlError,
};

/// 通知消息
///
/// 对应 XML 格式：
/// ```xml
/// <?xml version="1.0" encoding="UTF-8"?>
/// <Notify>
///   <CmdType>Keepalive</CmdType>
///   <SN>1</SN>
///   <DeviceID>34020000001320000001</DeviceID>
///   <Status>OK</Status>
/// </Notify>
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Notify {
    /// 命令类型
    pub cmd_type: CmdType,
    /// 命令序号
    pub sn: u32,
    /// 目标设备编码
    pub device_id: DeviceId,
    /// 心跳状态（Keepalive 时使用，如 "OK"）
    pub status: Option<String>,
    /// 报警列表（Alarm 时使用）
    pub alarm_list: Vec<AlarmInfo>,
    /// 移动位置列表（MobilePositionNotify 时使用）
    pub mobile_position_list: Vec<MobilePositionInfo>,
}

impl Notify {
    /// 创建心跳保活通知
    pub fn keepalive(sn: u32, device_id: DeviceId) -> Self {
        Self {
            cmd_type: CmdType::Keepalive,
            sn,
            device_id,
            status: Some("OK".to_string()),
            alarm_list: Vec::new(),
            mobile_position_list: Vec::new(),
        }
    }

    /// 创建报警通知
    pub fn alarm(sn: u32, device_id: DeviceId, alarms: Vec<AlarmInfo>) -> Self {
        Self {
            cmd_type: CmdType::Alarm,
            sn,
            device_id,
            status: None,
            alarm_list: alarms,
            mobile_position_list: Vec::new(),
        }
    }

    /// 创建移动位置通知
    pub fn mobile_position_notify(
        sn: u32,
        device_id: DeviceId,
        positions: Vec<MobilePositionInfo>,
    ) -> Self {
        Self {
            cmd_type: CmdType::MobilePositionNotify,
            sn,
            device_id,
            status: None,
            alarm_list: Vec::new(),
            mobile_position_list: positions,
        }
    }

    /// 将通知消息序列化为 XML 字符串
    pub fn to_xml(&self) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Notify>\n  <CmdType>{}</CmdType>\n  <SN>{}</SN>\n  <DeviceID>{}</DeviceID>",
            self.cmd_type.as_str(),
            self.sn,
            self.device_id,
        );

        if let Some(ref status) = self.status {
            xml.push_str(&format!("\n  <Status>{}</Status>", escape_xml(status)));
        }

        if !self.alarm_list.is_empty() {
            xml.push_str(&format!(
                "\n  <AlarmList Num=\"{}\">",
                self.alarm_list.len()
            ));
            for item in &self.alarm_list {
                xml.push_str(&item.to_xml_item());
            }
            xml.push_str("  </AlarmList>\n");
        }

        if !self.mobile_position_list.is_empty() {
            xml.push_str(&format!(
                "\n  <MobilePositionList Num=\"{}\">",
                self.mobile_position_list.len()
            ));
            for item in &self.mobile_position_list {
                xml.push_str(&item.to_xml_item());
            }
            xml.push_str("  </MobilePositionList>\n");
        }

        xml.push_str("\n</Notify>");
        xml
    }

    /// 从 XML 字符串解析通知消息
    pub fn from_xml(xml: &str) -> Result<Self, XmlError> {
        let body = strip_xml_declaration(xml);

        // 验证根元素
        let inner = extract_between_tags(body, "Notify")
            .ok_or_else(|| XmlError::InvalidFormat("missing <Notify> root element".to_string()))?;

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

        let status = extract_tag_value(&inner, "Status").map(|v| unescape_xml(&v));

        // 解析报警列表
        let alarm_list =
            if let Some((_attrs, list_inner)) = extract_tag_with_attrs(&inner, "AlarmList") {
                let item_contents = extract_all_tags(&list_inner, "Item");
                let mut items = Vec::with_capacity(item_contents.len());
                for item_content in &item_contents {
                    items.push(AlarmInfo::from_xml_item(item_content)?);
                }
                items
            } else {
                Vec::new()
            };

        // 解析移动位置列表
        let mobile_position_list = if let Some((_attrs, list_inner)) =
            extract_tag_with_attrs(&inner, "MobilePositionList")
        {
            let item_contents = extract_all_tags(&list_inner, "Item");
            let mut items = Vec::with_capacity(item_contents.len());
            for item_content in &item_contents {
                items.push(MobilePositionInfo::from_xml_item(item_content)?);
            }
            items
        } else {
            Vec::new()
        };

        Ok(Self {
            cmd_type,
            sn,
            device_id,
            status,
            alarm_list,
            mobile_position_list,
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
    fn test_keepalive_notify_to_xml() {
        let device_id = make_device_id("34020000001320000001");
        let notify = Notify::keepalive(1, device_id);
        let xml = notify.to_xml();

        assert!(xml.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<Notify>"));
        assert!(xml.contains("<CmdType>Keepalive</CmdType>"));
        assert!(xml.contains("<SN>1</SN>"));
        assert!(xml.contains("<DeviceID>34020000001320000001</DeviceID>"));
        assert!(xml.contains("<Status>OK</Status>"));
        assert!(xml.contains("</Notify>"));
    }

    #[test]
    fn test_keepalive_notify_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify>
  <CmdType>Keepalive</CmdType>
  <SN>1</SN>
  <DeviceID>34020000001320000001</DeviceID>
  <Status>OK</Status>
</Notify>"#;

        let notify = Notify::from_xml(xml).unwrap();
        assert_eq!(notify.cmd_type, CmdType::Keepalive);
        assert_eq!(notify.sn, 1);
        assert_eq!(notify.device_id.as_str(), "34020000001320000001");
        assert_eq!(notify.status.as_deref(), Some("OK"));
    }

    #[test]
    fn test_keepalive_notify_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let original = Notify::keepalive(42, device_id);
        let xml = original.to_xml();
        let parsed = Notify::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, original.cmd_type);
        assert_eq!(parsed.sn, original.sn);
        assert_eq!(parsed.device_id, original.device_id);
        assert_eq!(parsed.status, original.status);
    }

    #[test]
    fn test_alarm_notify_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let mut alarm1 = AlarmInfo::new("34020000001320000001", "1", "2024-01-01T12:00:00");
        alarm1.alarm_description = Some("运动检测".to_string());
        let alarm2 = AlarmInfo::new("34020000001320000002", "2", "2024-01-01T13:00:00");

        let original = Notify::alarm(5, device_id, vec![alarm1, alarm2]);
        let xml = original.to_xml();
        let parsed = Notify::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::Alarm);
        assert_eq!(parsed.alarm_list.len(), 2);
        assert_eq!(
            parsed.alarm_list[0].alarm_description.as_deref(),
            Some("运动检测")
        );
    }

    #[test]
    fn test_mobile_position_notify_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let mut pos1 = MobilePositionInfo::new(
            "34020000001320000001",
            116.397,
            39.908,
            "2024-01-01T12:00:00",
        );
        pos1.altitude = Some(50.0);
        pos1.speed = Some(60.5);
        pos1.direction = Some(180.0);

        let original = Notify::mobile_position_notify(10, device_id, vec![pos1]);
        let xml = original.to_xml();
        let parsed = Notify::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::MobilePositionNotify);
        assert_eq!(parsed.mobile_position_list.len(), 1);
        let pos = &parsed.mobile_position_list[0];
        assert!((pos.longitude - 116.397).abs() < f64::EPSILON);
        assert!((pos.latitude - 39.908).abs() < f64::EPSILON);
        assert_eq!(pos.altitude, Some(50.0));
        assert_eq!(pos.speed, Some(60.5));
        assert_eq!(pos.direction, Some(180.0));
    }

    #[test]
    fn test_notify_from_xml_missing_field() {
        let xml = "<Notify><CmdType>Keepalive</CmdType></Notify>";
        let result = Notify::from_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_notify_from_xml_no_root() {
        let xml = "<Other><CmdType>Keepalive</CmdType></Other>";
        let result = Notify::from_xml(xml);
        assert!(result.is_err());
    }
}
