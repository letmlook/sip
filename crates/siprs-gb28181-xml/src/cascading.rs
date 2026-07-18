//! GB28181 级联注册消息
//!
//! 对应 GB28181 级联平台之间的注册 XML 消息。

use crate::types::{
    escape_xml, extract_tag_value, strip_xml_declaration, unescape_xml, CmdType, XmlError,
};

/// 级联注册消息
///
/// 用于下级平台向上级平台注册，或上级平台处理下级平台的注册请求。
///
/// 对应 XML 格式：
/// ```xml
/// <?xml version="1.0" encoding="UTF-8"?>
/// <Command>
///   <CmdType>CascadingRegister</CmdType>
///   <SN>1</SN>
///   <DeviceID>34020000002000000001</DeviceID>
///   <ServerID>34020000002000000001</ServerID>
///   <ServerDomain>3402000000</ServerDomain>
///   <ServerIP>192.168.1.1</ServerIP>
///   <ServerPort>5060</ServerPort>
/// </Command>
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct CascadingRegisterXml {
    /// 命令序号
    pub sn: u32,
    /// 目标设备编码（上级平台编码）
    pub device_id: String,
    /// 下级平台编码
    pub server_id: String,
    /// 下级平台域名
    pub server_domain: String,
    /// 下级平台 IP
    pub server_ip: String,
    /// 下级平台端口
    pub server_port: u16,
}

impl CascadingRegisterXml {
    /// 创建级联注册消息
    pub fn new(
        sn: u32,
        device_id: impl Into<String>,
        server_id: impl Into<String>,
        server_domain: impl Into<String>,
        server_ip: impl Into<String>,
        server_port: u16,
    ) -> Self {
        Self {
            sn,
            device_id: device_id.into(),
            server_id: server_id.into(),
            server_domain: server_domain.into(),
            server_ip: server_ip.into(),
            server_port,
        }
    }

    /// 将级联注册消息序列化为 XML 字符串
    pub fn to_xml(&self) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <Command>\n\
             <CmdType>CascadingRegister</CmdType>\n\
             <SN>{}</SN>\n\
             <DeviceID>{}</DeviceID>\n\
             <ServerID>{}</ServerID>\n\
             <ServerDomain>{}</ServerDomain>\n\
             <ServerIP>{}</ServerIP>\n\
             <ServerPort>{}</ServerPort>\n\
             </Command>",
            self.sn,
            escape_xml(&self.device_id),
            escape_xml(&self.server_id),
            escape_xml(&self.server_domain),
            escape_xml(&self.server_ip),
            self.server_port,
        )
    }

    /// 从 XML 字符串解析级联注册消息
    pub fn from_xml(xml: &str) -> Result<Self, XmlError> {
        let body = strip_xml_declaration(xml);

        // 验证根元素
        let inner = extract_between_tags(body, "Command")
            .ok_or_else(|| XmlError::InvalidFormat("missing <Command> root element".to_string()))?;

        // 验证 CmdType
        let cmd_type_str = extract_tag_value(&inner, "CmdType")
            .ok_or(XmlError::MissingField("CmdType".to_string()))?;
        let cmd_type: CmdType = unescape_xml(&cmd_type_str).parse().unwrap();
        if !matches!(cmd_type, CmdType::Other(ref s) if s == "CascadingRegister") {
            return Err(XmlError::InvalidFormat(format!(
                "expected CmdType CascadingRegister, got {}",
                cmd_type.as_str()
            )));
        }

        let sn_str =
            extract_tag_value(&inner, "SN").ok_or(XmlError::MissingField("SN".to_string()))?;
        let sn: u32 = sn_str
            .parse()
            .map_err(|_| XmlError::InvalidNumber(format!("invalid SN: {sn_str}")))?;

        let device_id = extract_tag_value(&inner, "DeviceID")
            .ok_or(XmlError::MissingField("DeviceID".to_string()))?;
        let server_id = extract_tag_value(&inner, "ServerID")
            .ok_or(XmlError::MissingField("ServerID".to_string()))?;
        let server_domain = extract_tag_value(&inner, "ServerDomain")
            .ok_or(XmlError::MissingField("ServerDomain".to_string()))?;
        let server_ip = extract_tag_value(&inner, "ServerIP")
            .ok_or(XmlError::MissingField("ServerIP".to_string()))?;
        let server_port_str = extract_tag_value(&inner, "ServerPort")
            .ok_or(XmlError::MissingField("ServerPort".to_string()))?;
        let server_port: u16 = server_port_str.parse().map_err(|_| {
            XmlError::InvalidNumber(format!("invalid ServerPort: {server_port_str}"))
        })?;

        Ok(Self {
            sn,
            device_id: unescape_xml(&device_id),
            server_id: unescape_xml(&server_id),
            server_domain: unescape_xml(&server_domain),
            server_ip: unescape_xml(&server_ip),
            server_port,
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

    #[test]
    fn test_cascading_register_to_xml() {
        let reg = CascadingRegisterXml::new(
            1,
            "34020000002000000001",
            "34020000002000000002",
            "3402000000",
            "192.168.1.2",
            5060,
        );
        let xml = reg.to_xml();

        assert!(xml.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<Command>"));
        assert!(xml.contains("<CmdType>CascadingRegister</CmdType>"));
        assert!(xml.contains("<SN>1</SN>"));
        assert!(xml.contains("<DeviceID>34020000002000000001</DeviceID>"));
        assert!(xml.contains("<ServerID>34020000002000000002</ServerID>"));
        assert!(xml.contains("<ServerDomain>3402000000</ServerDomain>"));
        assert!(xml.contains("<ServerIP>192.168.1.2</ServerIP>"));
        assert!(xml.contains("<ServerPort>5060</ServerPort>"));
        assert!(xml.contains("</Command>"));
    }

    #[test]
    fn test_cascading_register_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Command>
  <CmdType>CascadingRegister</CmdType>
  <SN>1</SN>
  <DeviceID>34020000002000000001</DeviceID>
  <ServerID>34020000002000000002</ServerID>
  <ServerDomain>3402000000</ServerDomain>
  <ServerIP>192.168.1.2</ServerIP>
  <ServerPort>5060</ServerPort>
</Command>"#;

        let reg = CascadingRegisterXml::from_xml(xml).unwrap();
        assert_eq!(reg.sn, 1);
        assert_eq!(reg.device_id, "34020000002000000001");
        assert_eq!(reg.server_id, "34020000002000000002");
        assert_eq!(reg.server_domain, "3402000000");
        assert_eq!(reg.server_ip, "192.168.1.2");
        assert_eq!(reg.server_port, 5060);
    }

    #[test]
    fn test_cascading_register_roundtrip() {
        let original = CascadingRegisterXml::new(
            42,
            "34020000002000000001",
            "34020000002000000002",
            "3402000000",
            "192.168.1.2",
            5060,
        );
        let xml = original.to_xml();
        let parsed = CascadingRegisterXml::from_xml(&xml).unwrap();

        assert_eq!(parsed.sn, original.sn);
        assert_eq!(parsed.device_id, original.device_id);
        assert_eq!(parsed.server_id, original.server_id);
        assert_eq!(parsed.server_domain, original.server_domain);
        assert_eq!(parsed.server_ip, original.server_ip);
        assert_eq!(parsed.server_port, original.server_port);
    }

    #[test]
    fn test_cascading_register_wrong_cmd_type() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Command>
  <CmdType>Catalog</CmdType>
  <SN>1</SN>
  <DeviceID>34020000002000000001</DeviceID>
</Command>"#;

        let result = CascadingRegisterXml::from_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_cascading_register_missing_field() {
        let xml = "<Command><CmdType>CascadingRegister</CmdType><SN>1</SN></Command>";
        let result = CascadingRegisterXml::from_xml(xml);
        assert!(result.is_err());
    }
}
