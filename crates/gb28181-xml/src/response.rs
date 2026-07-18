//! GB28181 响应消息（Response）
//!
//! 对应 XML 根元素 `<Response>`，用于设备目录响应、设备信息响应、录像查询响应等。

use gb28181_codec::DeviceId;

use crate::types::{
    extract_all_tags, extract_tag_value, extract_tag_with_attrs, strip_xml_declaration,
    unescape_xml, AlarmInfo, CmdType, DeviceItem, DeviceStatusInfo, MobilePositionInfo, RecordItem,
    XmlError,
};

/// 响应消息
///
/// 对应 XML 格式：
/// ```xml
/// <?xml version="1.0" encoding="UTF-8"?>
/// <Response>
///   <CmdType>Catalog</CmdType>
///   <SN>1</SN>
///   <DeviceID>34020000002000000001</DeviceID>
///   <SumNum>2</SumNum>
///   <DeviceList Num="2">
///     <Item>...</Item>
///   </DeviceList>
/// </Response>
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    /// 命令类型
    pub cmd_type: CmdType,
    /// 命令序号
    pub sn: u32,
    /// 目标设备编码
    pub device_id: DeviceId,
    /// 设备总数（目录响应/录像响应时使用）
    pub sum_num: Option<u32>,
    /// 设备列表（目录响应时使用）
    pub device_list: Vec<DeviceItem>,
    /// 录像列表（录像查询响应时使用）
    pub record_list: Vec<RecordItem>,
    /// 报警列表（报警通知时使用）
    pub alarm_list: Vec<AlarmInfo>,
    /// 移动位置列表（移动位置通知时使用）
    pub mobile_position_list: Vec<MobilePositionInfo>,
    /// 设备状态信息（设备状态响应时使用）
    pub device_status: Option<DeviceStatusInfo>,
}

impl Response {
    /// 创建设备目录响应
    pub fn catalog(
        sn: u32,
        device_id: DeviceId,
        sum_num: u32,
        device_list: Vec<DeviceItem>,
    ) -> Self {
        Self {
            cmd_type: CmdType::Catalog,
            sn,
            device_id,
            sum_num: Some(sum_num),
            device_list,
            record_list: Vec::new(),
            alarm_list: Vec::new(),
            mobile_position_list: Vec::new(),
            device_status: None,
        }
    }

    /// 创建设备信息响应
    pub fn device_info(sn: u32, device_id: DeviceId) -> Self {
        Self {
            cmd_type: CmdType::DeviceInfo,
            sn,
            device_id,
            sum_num: None,
            device_list: Vec::new(),
            record_list: Vec::new(),
            alarm_list: Vec::new(),
            mobile_position_list: Vec::new(),
            device_status: None,
        }
    }

    /// 创建录像查询响应
    pub fn record_query(sn: u32, device_id: DeviceId, records: Vec<RecordItem>) -> Self {
        let sum_num = records.len() as u32;
        Self {
            cmd_type: CmdType::RecordQuery,
            sn,
            device_id,
            sum_num: Some(sum_num),
            device_list: Vec::new(),
            record_list: records,
            alarm_list: Vec::new(),
            mobile_position_list: Vec::new(),
            device_status: None,
        }
    }

    /// 创建设备状态响应
    pub fn device_status(sn: u32, device_id: DeviceId, status: DeviceStatusInfo) -> Self {
        Self {
            cmd_type: CmdType::DeviceStatus,
            sn,
            device_id,
            sum_num: None,
            device_list: Vec::new(),
            record_list: Vec::new(),
            alarm_list: Vec::new(),
            mobile_position_list: Vec::new(),
            device_status: Some(status),
        }
    }

    /// 创建报警通知响应
    pub fn alarm(sn: u32, device_id: DeviceId, alarms: Vec<AlarmInfo>) -> Self {
        Self {
            cmd_type: CmdType::Alarm,
            sn,
            device_id,
            sum_num: Some(alarms.len() as u32),
            device_list: Vec::new(),
            record_list: Vec::new(),
            alarm_list: alarms,
            mobile_position_list: Vec::new(),
            device_status: None,
        }
    }

    /// 创建移动位置通知响应
    pub fn mobile_position_notify(
        sn: u32,
        device_id: DeviceId,
        positions: Vec<MobilePositionInfo>,
    ) -> Self {
        Self {
            cmd_type: CmdType::MobilePositionNotify,
            sn,
            device_id,
            sum_num: Some(positions.len() as u32),
            device_list: Vec::new(),
            record_list: Vec::new(),
            alarm_list: Vec::new(),
            mobile_position_list: positions,
            device_status: None,
        }
    }

    /// 将响应消息序列化为 XML 字符串
    pub fn to_xml(&self) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Response>\n  <CmdType>{}</CmdType>\n  <SN>{}</SN>\n  <DeviceID>{}</DeviceID>",
            self.cmd_type.as_str(),
            self.sn,
            self.device_id,
        );

        if let Some(sum_num) = self.sum_num {
            xml.push_str(&format!("\n  <SumNum>{sum_num}</SumNum>"));
        }

        if !self.device_list.is_empty() {
            xml.push_str(&format!(
                "\n  <DeviceList Num=\"{}\">",
                self.device_list.len()
            ));
            for item in &self.device_list {
                xml.push_str(&item.to_xml_item());
            }
            xml.push_str("  </DeviceList>\n");
        }

        if !self.record_list.is_empty() {
            xml.push_str(&format!(
                "\n  <RecordList Num=\"{}\">",
                self.record_list.len()
            ));
            for item in &self.record_list {
                xml.push_str(&item.to_xml_item());
            }
            xml.push_str("  </RecordList>\n");
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

        if let Some(ref status) = self.device_status {
            xml.push_str(&status.to_xml_elements());
        }

        xml.push_str("</Response>");
        xml
    }

    /// 从 XML 字符串解析响应消息
    pub fn from_xml(xml: &str) -> Result<Self, XmlError> {
        let body = strip_xml_declaration(xml);

        // 验证根元素并提取内容
        let inner = extract_between_tags(body, "Response").ok_or_else(|| {
            XmlError::InvalidFormat("missing <Response> root element".to_string())
        })?;

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

        let sum_num = extract_tag_value(&inner, "SumNum").and_then(|v| v.parse().ok());

        // 解析设备列表
        let device_list =
            if let Some((_attrs, list_inner)) = extract_tag_with_attrs(&inner, "DeviceList") {
                let item_contents = extract_all_tags(&list_inner, "Item");
                let mut items = Vec::with_capacity(item_contents.len());
                for item_content in &item_contents {
                    items.push(DeviceItem::from_xml_item(item_content)?);
                }
                items
            } else {
                Vec::new()
            };

        // 解析录像列表
        let record_list =
            if let Some((_attrs, list_inner)) = extract_tag_with_attrs(&inner, "RecordList") {
                let item_contents = extract_all_tags(&list_inner, "Item");
                let mut items = Vec::with_capacity(item_contents.len());
                for item_content in &item_contents {
                    items.push(RecordItem::from_xml_item(item_content)?);
                }
                items
            } else {
                Vec::new()
            };

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

        // 解析设备状态信息
        let device_status = if inner.contains("<Online>") {
            Some(DeviceStatusInfo::from_xml_elements(&inner)?)
        } else {
            None
        };

        Ok(Self {
            cmd_type,
            sn,
            device_id,
            sum_num,
            device_list,
            record_list,
            alarm_list,
            mobile_position_list,
            device_status,
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
    fn test_catalog_response_to_xml() {
        let device_id = make_device_id("34020000002000000001");
        let item1_id = make_device_id("34020000001320000001");
        let item2_id = make_device_id("34020000001320000002");

        let mut item1 = DeviceItem::new(item1_id);
        item1.name = Some("Camera1".to_string());
        item1.manufacturer = Some("Hikvision".to_string());
        item1.status = Some("ON".to_string());

        let mut item2 = DeviceItem::new(item2_id);
        item2.name = Some("Camera2".to_string());
        item2.status = Some("OFF".to_string());

        let response = Response::catalog(1, device_id, 2, vec![item1, item2]);
        let xml = response.to_xml();

        assert!(xml.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<Response>"));
        assert!(xml.contains("<CmdType>Catalog</CmdType>"));
        assert!(xml.contains("<SN>1</SN>"));
        assert!(xml.contains("<DeviceID>34020000002000000001</DeviceID>"));
        assert!(xml.contains("<SumNum>2</SumNum>"));
        assert!(xml.contains("<DeviceList Num=\"2\">"));
        assert!(xml.contains("<DeviceID>34020000001320000001</DeviceID>"));
        assert!(xml.contains("<Name>Camera1</Name>"));
        assert!(xml.contains("<Manufacturer>Hikvision</Manufacturer>"));
        assert!(xml.contains("<Status>ON</Status>"));
        assert!(xml.contains("<DeviceID>34020000001320000002</DeviceID>"));
        assert!(xml.contains("<Name>Camera2</Name>"));
        assert!(xml.contains("<Status>OFF</Status>"));
        assert!(xml.contains("</DeviceList>"));
        assert!(xml.contains("</Response>"));
    }

    #[test]
    fn test_catalog_response_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <CmdType>Catalog</CmdType>
  <SN>1</SN>
  <DeviceID>34020000002000000001</DeviceID>
  <SumNum>2</SumNum>
  <DeviceList Num="2">
    <Item>
      <DeviceID>34020000001320000001</DeviceID>
      <Name>Camera1</Name>
      <Manufacturer>Hikvision</Manufacturer>
      <Model>DS-2CD2143</Model>
      <Owner>Owner1</Owner>
      <CivilCode>340200</CivilCode>
      <Block>Block1</Block>
      <Address>Address1</Address>
      <Parental>0</Parental>
      <ParentID>34020000002000000001</ParentID>
      <SafetyWay>0</SafetyWay>
      <RegisterWay>1</RegisterWay>
      <CertNum></CertNum>
      <Certifiable>0</Certifiable>
      <ErrCode>0</ErrCode>
      <EndTime>2024-01-01T00:00:00</EndTime>
      <Secrecy>0</Secrecy>
      <IPAddress>192.168.1.100</IPAddress>
      <Port>5060</Port>
      <Password></Password>
      <PTZType>0</PTZType>
      <Status>ON</Status>
      <Longitude>0.0</Longitude>
      <Latitude>0.0</Latitude>
    </Item>
    <Item>
      <DeviceID>34020000001320000002</DeviceID>
      <Name>Camera2</Name>
      <Status>OFF</Status>
    </Item>
  </DeviceList>
</Response>"#;

        let response = Response::from_xml(xml).unwrap();
        assert_eq!(response.cmd_type, CmdType::Catalog);
        assert_eq!(response.sn, 1);
        assert_eq!(response.device_id.as_str(), "34020000002000000001");
        assert_eq!(response.sum_num, Some(2));
        assert_eq!(response.device_list.len(), 2);

        let item1 = &response.device_list[0];
        assert_eq!(item1.device_id.as_str(), "34020000001320000001");
        assert_eq!(item1.name.as_deref(), Some("Camera1"));
        assert_eq!(item1.manufacturer.as_deref(), Some("Hikvision"));
        assert_eq!(item1.model.as_deref(), Some("DS-2CD2143"));
        assert_eq!(item1.owner.as_deref(), Some("Owner1"));
        assert_eq!(item1.civil_code.as_deref(), Some("340200"));
        assert_eq!(item1.block.as_deref(), Some("Block1"));
        assert_eq!(item1.address.as_deref(), Some("Address1"));
        assert_eq!(item1.parental, Some(0));
        assert_eq!(item1.parent_id.as_deref(), Some("34020000002000000001"));
        assert_eq!(item1.safety_way, Some(0));
        assert_eq!(item1.register_way, Some(1));
        assert_eq!(item1.cert_num.as_deref(), Some(""));
        assert_eq!(item1.certifiable, Some(0));
        assert_eq!(item1.err_code, Some(0));
        assert_eq!(item1.end_time.as_deref(), Some("2024-01-01T00:00:00"));
        assert_eq!(item1.secrecy, Some(0));
        assert_eq!(item1.ip_address.as_deref(), Some("192.168.1.100"));
        assert_eq!(item1.port, Some(5060));
        assert_eq!(item1.password.as_deref(), Some(""));
        assert_eq!(item1.ptz_type, Some(0));
        assert_eq!(item1.status.as_deref(), Some("ON"));
        assert_eq!(item1.longitude, Some(0.0));
        assert_eq!(item1.latitude, Some(0.0));

        let item2 = &response.device_list[1];
        assert_eq!(item2.device_id.as_str(), "34020000001320000002");
        assert_eq!(item2.name.as_deref(), Some("Camera2"));
        assert_eq!(item2.status.as_deref(), Some("OFF"));
        assert_eq!(item2.manufacturer, None);
    }

    #[test]
    fn test_catalog_response_roundtrip() {
        let device_id = make_device_id("34020000002000000001");
        let item1_id = make_device_id("34020000001320000001");
        let item2_id = make_device_id("34020000001320000002");

        let mut item1 = DeviceItem::new(item1_id);
        item1.name = Some("Camera1".to_string());
        item1.manufacturer = Some("Hikvision".to_string());
        item1.model = Some("DS-2CD2143".to_string());
        item1.status = Some("ON".to_string());
        item1.longitude = Some(116.397);
        item1.latitude = Some(39.908);

        let mut item2 = DeviceItem::new(item2_id);
        item2.name = Some("Camera2".to_string());
        item2.status = Some("OFF".to_string());

        let original = Response::catalog(1, device_id, 2, vec![item1, item2]);
        let xml = original.to_xml();
        let parsed = Response::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, original.cmd_type);
        assert_eq!(parsed.sn, original.sn);
        assert_eq!(parsed.device_id, original.device_id);
        assert_eq!(parsed.sum_num, original.sum_num);
        assert_eq!(parsed.device_list.len(), original.device_list.len());

        // 验证设备项内容一致性
        for (p, o) in parsed.device_list.iter().zip(original.device_list.iter()) {
            assert_eq!(p.device_id, o.device_id);
            assert_eq!(p.name, o.name);
            assert_eq!(p.manufacturer, o.manufacturer);
            assert_eq!(p.status, o.status);
        }

        // 二次往返验证
        let xml2 = parsed.to_xml();
        let parsed2 = Response::from_xml(&xml2).unwrap();
        assert_eq!(parsed2.cmd_type, parsed.cmd_type);
        assert_eq!(parsed2.sn, parsed.sn);
        assert_eq!(parsed2.device_id, parsed.device_id);
        assert_eq!(parsed2.sum_num, parsed.sum_num);
        assert_eq!(parsed2.device_list.len(), parsed.device_list.len());
    }

    #[test]
    fn test_response_empty_device_list() {
        let device_id = make_device_id("34020000002000000001");
        let response = Response::catalog(1, device_id, 0, vec![]);
        let xml = response.to_xml();
        let parsed = Response::from_xml(&xml).unwrap();

        assert_eq!(parsed.sum_num, Some(0));
        assert!(parsed.device_list.is_empty());
    }

    #[test]
    fn test_record_query_response_to_xml() {
        let device_id = make_device_id("34020000002000000001");
        let records = vec![
            RecordItem::new(
                "34020000001320000001",
                "录像1",
                "2024-01-01T00:00:00",
                "2024-01-01T23:59:59",
            ),
            RecordItem::new(
                "34020000001320000002",
                "录像2",
                "2024-01-02T00:00:00",
                "2024-01-02T23:59:59",
            ),
        ];
        let response = Response::record_query(10, device_id, records);
        let xml = response.to_xml();

        assert!(xml.contains("<CmdType>RecordQuery</CmdType>"));
        assert!(xml.contains("<SN>10</SN>"));
        assert!(xml.contains("<SumNum>2</SumNum>"));
        assert!(xml.contains("<RecordList Num=\"2\">"));
        assert!(xml.contains("<DeviceID>34020000001320000001</DeviceID>"));
        assert!(xml.contains("<Name>录像1</Name>"));
        assert!(xml.contains("<StartTime>2024-01-01T00:00:00</StartTime>"));
        assert!(xml.contains("<EndTime>2024-01-01T23:59:59</EndTime>"));
        assert!(xml.contains("</RecordList>"));
    }

    #[test]
    fn test_record_query_response_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <CmdType>RecordQuery</CmdType>
  <SN>10</SN>
  <DeviceID>34020000002000000001</DeviceID>
  <SumNum>1</SumNum>
  <RecordList Num="1">
    <Item>
      <DeviceID>34020000001320000001</DeviceID>
      <Name>录像1</Name>
      <StartTime>2024-01-01T00:00:00</StartTime>
      <EndTime>2024-01-01T23:59:59</EndTime>
    </Item>
  </RecordList>
</Response>"#;

        let response = Response::from_xml(xml).unwrap();
        assert_eq!(response.cmd_type, CmdType::RecordQuery);
        assert_eq!(response.sn, 10);
        assert_eq!(response.sum_num, Some(1));
        assert_eq!(response.record_list.len(), 1);

        let record = &response.record_list[0];
        assert_eq!(record.device_id, "34020000001320000001");
        assert_eq!(record.name, "录像1");
        assert_eq!(record.start_time, "2024-01-01T00:00:00");
        assert_eq!(record.end_time, "2024-01-01T23:59:59");
    }

    #[test]
    fn test_record_query_response_roundtrip() {
        let device_id = make_device_id("34020000002000000001");
        let records = vec![RecordItem::new(
            "34020000001320000001",
            "录像1",
            "2024-01-01T00:00:00",
            "2024-01-01T23:59:59",
        )];
        let original = Response::record_query(10, device_id, records);
        let xml = original.to_xml();
        let parsed = Response::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::RecordQuery);
        assert_eq!(parsed.sn, 10);
        assert_eq!(parsed.sum_num, Some(1));
        assert_eq!(parsed.record_list.len(), 1);
        assert_eq!(parsed.record_list[0].device_id, "34020000001320000001");
        assert_eq!(parsed.record_list[0].name, "录像1");
        assert_eq!(parsed.record_list[0].start_time, "2024-01-01T00:00:00");
        assert_eq!(parsed.record_list[0].end_time, "2024-01-01T23:59:59");
    }

    #[test]
    fn test_device_status_response_to_xml() {
        let device_id = make_device_id("34020000001320000001");
        let mut status = DeviceStatusInfo::new(true, "OK");
        status.encode = Some(true);
        status.record = Some(false);
        status.device_time = Some("2024-01-01T12:00:00".to_string());
        let response = Response::device_status(5, device_id, status);
        let xml = response.to_xml();

        assert!(xml.contains("<CmdType>DeviceStatus</CmdType>"));
        assert!(xml.contains("<SN>5</SN>"));
        assert!(xml.contains("<Result>OK</Result>"));
        assert!(xml.contains("<Online>ONLINE</Online>"));
        assert!(xml.contains("<Status>OK</Status>"));
        assert!(xml.contains("<Encode>ON</Encode>"));
        assert!(xml.contains("<Record>OFF</Record>"));
        assert!(xml.contains("<DeviceTime>2024-01-01T12:00:00</DeviceTime>"));
    }

    #[test]
    fn test_device_status_response_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <CmdType>DeviceStatus</CmdType>
  <SN>5</SN>
  <DeviceID>34020000001320000001</DeviceID>
  <Result>OK</Result>
  <Online>ONLINE</Online>
  <Status>OK</Status>
  <Encode>ON</Encode>
  <Record>OFF</Record>
  <DeviceTime>2024-01-01T12:00:00</DeviceTime>
</Response>"#;

        let response = Response::from_xml(xml).unwrap();
        assert_eq!(response.cmd_type, CmdType::DeviceStatus);
        assert_eq!(response.sn, 5);
        let status = response.device_status.unwrap();
        assert!(status.online);
        assert_eq!(status.status, "OK");
        assert_eq!(status.encode, Some(true));
        assert_eq!(status.record, Some(false));
        assert_eq!(status.device_time.as_deref(), Some("2024-01-01T12:00:00"));
    }

    #[test]
    fn test_device_status_response_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let mut status = DeviceStatusInfo::new(true, "OK");
        status.encode = Some(true);
        status.record = Some(false);
        status.device_time = Some("2024-01-01T12:00:00".to_string());
        let original = Response::device_status(5, device_id, status);
        let xml = original.to_xml();
        let parsed = Response::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::DeviceStatus);
        assert_eq!(parsed.sn, 5);
        let parsed_status = parsed.device_status.unwrap();
        assert!(parsed_status.online);
        assert_eq!(parsed_status.status, "OK");
        assert_eq!(parsed_status.encode, Some(true));
        assert_eq!(parsed_status.record, Some(false));
        assert_eq!(
            parsed_status.device_time.as_deref(),
            Some("2024-01-01T12:00:00")
        );
    }

    #[test]
    fn test_alarm_response_to_xml() {
        let device_id = make_device_id("34020000002000000001");
        let mut alarm1 = AlarmInfo::new("34020000001320000001", "1", "2024-01-01T12:00:00");
        alarm1.alarm_description = Some("运动检测报警".to_string());
        let alarm2 = AlarmInfo::new("34020000001320000002", "2", "2024-01-01T13:00:00");
        let response = Response::alarm(20, device_id, vec![alarm1, alarm2]);
        let xml = response.to_xml();

        assert!(xml.contains("<CmdType>Alarm</CmdType>"));
        assert!(xml.contains("<SN>20</SN>"));
        assert!(xml.contains("<SumNum>2</SumNum>"));
        assert!(xml.contains("<AlarmList Num=\"2\">"));
        assert!(xml.contains("<AlarmMethod>1</AlarmMethod>"));
        assert!(xml.contains("<AlarmTime>2024-01-01T12:00:00</AlarmTime>"));
        assert!(xml.contains("<AlarmDescription>运动检测报警</AlarmDescription>"));
        assert!(xml.contains("</AlarmList>"));
    }

    #[test]
    fn test_alarm_response_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <CmdType>Alarm</CmdType>
  <SN>20</SN>
  <DeviceID>34020000002000000001</DeviceID>
  <SumNum>1</SumNum>
  <AlarmList Num="1">
    <Item>
      <DeviceID>34020000001320000001</DeviceID>
      <AlarmMethod>1</AlarmMethod>
      <AlarmTime>2024-01-01T12:00:00</AlarmTime>
      <AlarmDescription>运动检测报警</AlarmDescription>
    </Item>
  </AlarmList>
</Response>"#;

        let response = Response::from_xml(xml).unwrap();
        assert_eq!(response.cmd_type, CmdType::Alarm);
        assert_eq!(response.sn, 20);
        assert_eq!(response.alarm_list.len(), 1);

        let alarm = &response.alarm_list[0];
        assert_eq!(alarm.device_id, "34020000001320000001");
        assert_eq!(alarm.alarm_method, "1");
        assert_eq!(alarm.alarm_time, "2024-01-01T12:00:00");
        assert_eq!(alarm.alarm_description.as_deref(), Some("运动检测报警"));
    }

    #[test]
    fn test_alarm_response_roundtrip() {
        let device_id = make_device_id("34020000002000000001");
        let mut alarm1 = AlarmInfo::new("34020000001320000001", "1", "2024-01-01T12:00:00");
        alarm1.alarm_description = Some("运动检测报警".to_string());
        let original = Response::alarm(20, device_id, vec![alarm1]);
        let xml = original.to_xml();
        let parsed = Response::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::Alarm);
        assert_eq!(parsed.alarm_list.len(), 1);
        assert_eq!(parsed.alarm_list[0].device_id, "34020000001320000001");
        assert_eq!(parsed.alarm_list[0].alarm_method, "1");
        assert_eq!(
            parsed.alarm_list[0].alarm_description.as_deref(),
            Some("运动检测报警")
        );
    }

    #[test]
    fn test_mobile_position_notify_to_xml() {
        let device_id = make_device_id("34020000002000000001");
        let mut pos = MobilePositionInfo::new(
            "34020000001320000001",
            116.397,
            39.908,
            "2024-01-01T12:00:00",
        );
        pos.speed = Some(60.5);
        let response = Response::mobile_position_notify(30, device_id, vec![pos]);
        let xml = response.to_xml();

        assert!(xml.contains("<CmdType>MobilePositionNotify</CmdType>"));
        assert!(xml.contains("<SN>30</SN>"));
        assert!(xml.contains("<SumNum>1</SumNum>"));
        assert!(xml.contains("<MobilePositionList Num=\"1\">"));
        assert!(xml.contains("<Longitude>116.397</Longitude>"));
        assert!(xml.contains("<Latitude>39.908</Latitude>"));
        assert!(xml.contains("<Speed>60.5</Speed>"));
        assert!(xml.contains("<ReportTime>2024-01-01T12:00:00</ReportTime>"));
        assert!(xml.contains("</MobilePositionList>"));
    }

    #[test]
    fn test_mobile_position_notify_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <CmdType>MobilePositionNotify</CmdType>
  <SN>30</SN>
  <DeviceID>34020000002000000001</DeviceID>
  <SumNum>1</SumNum>
  <MobilePositionList Num="1">
    <Item>
      <DeviceID>34020000001320000001</DeviceID>
      <Longitude>116.397</Longitude>
      <Latitude>39.908</Latitude>
      <Speed>60.5</Speed>
      <ReportTime>2024-01-01T12:00:00</ReportTime>
    </Item>
  </MobilePositionList>
</Response>"#;

        let response = Response::from_xml(xml).unwrap();
        assert_eq!(response.cmd_type, CmdType::MobilePositionNotify);
        assert_eq!(response.sn, 30);
        assert_eq!(response.mobile_position_list.len(), 1);

        let pos = &response.mobile_position_list[0];
        assert_eq!(pos.device_id, "34020000001320000001");
        assert!((pos.longitude - 116.397).abs() < f64::EPSILON);
        assert!((pos.latitude - 39.908).abs() < f64::EPSILON);
        assert_eq!(pos.speed, Some(60.5));
        assert_eq!(pos.report_time, "2024-01-01T12:00:00");
    }

    #[test]
    fn test_mobile_position_notify_roundtrip() {
        let device_id = make_device_id("34020000002000000001");
        let mut pos = MobilePositionInfo::new(
            "34020000001320000001",
            116.397,
            39.908,
            "2024-01-01T12:00:00",
        );
        pos.altitude = Some(50.0);
        pos.speed = Some(60.5);
        pos.direction = Some(180.0);
        let original = Response::mobile_position_notify(30, device_id, vec![pos]);
        let xml = original.to_xml();
        let parsed = Response::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::MobilePositionNotify);
        assert_eq!(parsed.mobile_position_list.len(), 1);
        let p = &parsed.mobile_position_list[0];
        assert_eq!(p.device_id, "34020000001320000001");
        assert!((p.longitude - 116.397).abs() < 0.001);
        assert!((p.latitude - 39.908).abs() < 0.001);
        assert_eq!(p.altitude, Some(50.0));
        assert_eq!(p.speed, Some(60.5));
        assert_eq!(p.direction, Some(180.0));
        assert_eq!(p.report_time, "2024-01-01T12:00:00");
    }

    #[test]
    fn test_response_from_xml_missing_root() {
        let xml = "<Other><CmdType>Catalog</CmdType></Other>";
        let result = Response::from_xml(xml);
        assert!(result.is_err());
    }
}
