//! GB28181 XML 正文处理库
//!
//! 实现 GB28181 MANSCDP XML 消息的解析和构建。
//!
//! # 支持的消息类型
//!
//! - **Query** - 查询消息（设备目录查询、设备信息查询、录像查询等）
//! - **Response** - 响应消息（设备目录响应、录像查询响应、报警通知等）
//! - **Control** - 控制消息（云台控制、远程启动、录像拖动等）
//!
//! # 示例
//!
//! ## 构建设备目录查询
//!
//! ```
//! use gb28181_codec::DeviceId;
//! use gb28181_xml::Query;
//!
//! let device_id = DeviceId::parse("34020000002000000001").unwrap();
//! let query = Query::catalog(1, device_id);
//! let xml = query.to_xml();
//! ```
//!
//! ## 构建录像查询
//!
//! ```
//! use gb28181_codec::DeviceId;
//! use gb28181_xml::Query;
//!
//! let device_id = DeviceId::parse("34020000001320000001").unwrap();
//! let query = Query::record_query(10, device_id, "2024-01-01T00:00:00", "2024-01-31T23:59:59");
//! let xml = query.to_xml();
//! ```
//!
//! ## 解析设备目录响应
//!
//! ```
//! use gb28181_xml::Response;
//!
//! let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
//! <Response>
//!   <CmdType>Catalog</CmdType>
//!   <SN>1</SN>
//!   <DeviceID>34020000002000000001</DeviceID>
//!   <SumNum>1</SumNum>
//!   <DeviceList Num="1">
//!     <Item>
//!       <DeviceID>34020000001320000001</DeviceID>
//!       <Name>Camera1</Name>
//!       <Status>ON</Status>
//!     </Item>
//!   </DeviceList>
//! </Response>"#;
//!
//! let response = Response::from_xml(xml).unwrap();
//! assert_eq!(response.device_list.len(), 1);
//! ```
//!
//! ## 构建云台控制命令
//!
//! ```
//! use gb28181_codec::DeviceId;
//! use gb28181_xml::Control;
//!
//! let device_id = DeviceId::parse("34020000001320000001").unwrap();
//! let control = Control::ptz(3, device_id, "A50F01001F0000");
//! let xml = control.to_xml();
//! ```
//!
//! ## 构建远程启动控制命令
//!
//! ```
//! use gb28181_codec::DeviceId;
//! use gb28181_xml::Control;
//!
//! let device_id = DeviceId::parse("34020000001320000001").unwrap();
//! let control = Control::remote_start(10, device_id);
//! let xml = control.to_xml();
//! ```

pub mod control;
pub mod query;
pub mod response;
pub mod types;

// 重导出核心类型，方便使用
pub use control::{build_ptz_cmd, Control, PtzDirection, PtzSpeed};
pub use query::Query;
pub use response::Response;
pub use types::{
    AlarmInfo, CmdType, DeviceItem, DeviceStatusInfo, MobilePositionInfo, RecordItem, XmlError,
};

/// 自动检测 XML 根元素并解析为对应的消息类型
///
/// 返回解析后的 `Message` 枚举。
pub fn parse_xml(xml: &str) -> Result<Message, XmlError> {
    let root = types::detect_root_element(xml)?;
    match root {
        "Query" => Query::from_xml(xml).map(Message::Query),
        "Response" => Response::from_xml(xml).map(Message::Response),
        "Control" => Control::from_xml(xml).map(Message::Control),
        other => Err(XmlError::UnknownRoot(other.to_string())),
    }
}

/// GB28181 XML 消息的统一枚举
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// 查询消息
    Query(Query),
    /// 响应消息
    Response(Response),
    /// 控制消息
    Control(Control),
}

#[cfg(test)]
mod tests {
    use super::*;
    use gb28181_codec::DeviceId;

    #[test]
    fn test_parse_xml_query() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Query>
  <CmdType>Catalog</CmdType>
  <SN>1</SN>
  <DeviceID>34020000002000000001</DeviceID>
</Query>"#;

        let msg = parse_xml(xml).unwrap();
        match msg {
            Message::Query(q) => {
                assert_eq!(q.cmd_type, CmdType::Catalog);
                assert_eq!(q.sn, 1);
            }
            _ => panic!("expected Query"),
        }
    }

    #[test]
    fn test_parse_xml_response() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
  <CmdType>Catalog</CmdType>
  <SN>1</SN>
  <DeviceID>34020000002000000001</DeviceID>
  <SumNum>0</SumNum>
</Response>"#;

        let msg = parse_xml(xml).unwrap();
        match msg {
            Message::Response(r) => {
                assert_eq!(r.cmd_type, CmdType::Catalog);
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn test_parse_xml_control() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Control>
  <CmdType>DeviceControl</CmdType>
  <SN>3</SN>
  <DeviceID>34020000001320000001</DeviceID>
  <PTZCmd>A50F01001F0000</PTZCmd>
</Control>"#;

        let msg = parse_xml(xml).unwrap();
        match msg {
            Message::Control(c) => {
                assert_eq!(c.cmd_type, CmdType::DeviceControl);
                assert_eq!(c.ptz_cmd.as_deref(), Some("A50F01001F0000"));
            }
            _ => panic!("expected Control"),
        }
    }

    #[test]
    fn test_parse_xml_unknown_root() {
        let xml = "<Unknown><CmdType>Catalog</CmdType></Unknown>";
        let result = parse_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_full_workflow() {
        // 构建查询
        let device_id = DeviceId::parse("34020000002000000001").unwrap();
        let query = Query::catalog(1, device_id.clone());
        let query_xml = query.to_xml();

        // 解析查询
        let parsed_query = Query::from_xml(&query_xml).unwrap();
        assert_eq!(parsed_query.cmd_type, CmdType::Catalog);

        // 构建响应
        let item_id = DeviceId::parse("34020000001320000001").unwrap();
        let mut item = DeviceItem::new(item_id);
        item.name = Some("Camera1".to_string());
        item.status = Some("ON".to_string());

        let response = Response::catalog(1, device_id, 1, vec![item]);
        let response_xml = response.to_xml();

        // 解析响应
        let parsed_response = Response::from_xml(&response_xml).unwrap();
        assert_eq!(parsed_response.device_list.len(), 1);
        assert_eq!(
            parsed_response.device_list[0].name.as_deref(),
            Some("Camera1")
        );

        // 构建控制
        let ctrl_device_id = DeviceId::parse("34020000001320000001").unwrap();
        let control = Control::ptz(3, ctrl_device_id, "A50F01001F0000");
        let control_xml = control.to_xml();

        // 解析控制
        let parsed_control = Control::from_xml(&control_xml).unwrap();
        assert_eq!(parsed_control.ptz_cmd.as_deref(), Some("A50F01001F0000"));
    }
}
