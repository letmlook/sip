//! GB28181 控制消息（Control）
//!
//! 对应 XML 根元素 `<Control>`，用于设备控制命令（如云台控制、远程启动、录像拖动等）。

use siprs_gb28181_codec::DeviceId;

use crate::types::{
    escape_xml, extract_tag_value, strip_xml_declaration, unescape_xml, CmdType, XmlError,
};

/// 云台控制命令方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtzDirection {
    /// 停止
    Stop,
    /// 上
    Up,
    /// 下
    Down,
    /// 左
    Left,
    /// 右
    Right,
    /// 左上
    LeftUp,
    /// 左下
    LeftDown,
    /// 右上
    RightUp,
    /// 右下
    RightDown,
}

impl PtzDirection {
    /// 获取方向对应的云台命令字节值（组合码）
    ///
    /// 返回 (水平方向, 垂直方向)：
    /// - 水平: 0=停止, 1=左, 2=右
    /// - 垂直: 0=停止, 1=下, 2=上
    fn as_byte_pair(self) -> (u8, u8) {
        match self {
            PtzDirection::Stop => (0, 0),
            PtzDirection::Up => (0, 2),
            PtzDirection::Down => (0, 1),
            PtzDirection::Left => (1, 0),
            PtzDirection::Right => (2, 0),
            PtzDirection::LeftUp => (1, 2),
            PtzDirection::LeftDown => (1, 1),
            PtzDirection::RightUp => (2, 2),
            PtzDirection::RightDown => (2, 1),
        }
    }
}

/// 云台控制速度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtzSpeed(u8);

impl PtzSpeed {
    /// 创建云台控制速度（0-255）
    pub fn new(speed: u8) -> Self {
        Self(speed)
    }

    /// 获取速度值
    pub fn value(self) -> u8 {
        self.0
    }
}

impl Default for PtzSpeed {
    fn default() -> Self {
        Self(0x1F) // 默认速度 31
    }
}

/// 生成 PTZ 控制命令的十六进制字符串
///
/// PTZCmd 格式（8字节，16位十六进制字符串）：
/// - Byte 0: A5（固定前缀）
/// - Byte 1: 组合码（低4位=水平方向, 高4位=垂直方向）
/// - Byte 2: 水平速度
/// - Byte 3: 垂直速度
/// - Byte 4-6: 预置位/巡航/轨迹（000000）
/// - Byte 7: 校验和（前7字节异或）
pub fn build_ptz_cmd(direction: PtzDirection, h_speed: PtzSpeed, v_speed: PtzSpeed) -> String {
    let (h_dir, v_dir) = direction.as_byte_pair();

    let byte0: u8 = 0xA5;
    let byte1: u8 = (v_dir << 4) | h_dir;
    let byte2: u8 = h_speed.value();
    let byte3: u8 = v_speed.value();
    let byte4: u8 = 0x00;
    let byte5: u8 = 0x00;
    let byte6: u8 = 0x00;
    let byte7: u8 = byte0 ^ byte1 ^ byte2 ^ byte3 ^ byte4 ^ byte5 ^ byte6;

    format!(
        "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        byte0, byte1, byte2, byte3, byte4, byte5, byte6, byte7
    )
}

/// 控制消息
///
/// 对应 XML 格式：
/// ```xml
/// <?xml version="1.0" encoding="UTF-8"?>
/// <Control>
///   <CmdType>DeviceControl</CmdType>
///   <SN>3</SN>
///   <DeviceID>34020000001320000001</DeviceID>
///   <PTZCmd>A50F01001F0000</PTZCmd>
/// </Control>
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    /// 命令类型
    pub cmd_type: CmdType,
    /// 命令序号
    pub sn: u32,
    /// 目标设备编码
    pub device_id: DeviceId,
    /// 云台控制命令（十六进制字符串）
    pub ptz_cmd: Option<String>,
    /// 远程启动标志
    pub tele_boot: Option<String>,
    /// 录像拖动时间（历史回放定位，格式: 2024-01-01T00:00:00）
    pub play_time: Option<String>,
    /// 报警方式
    pub alarm_method: Option<String>,
}

impl Control {
    /// 创建云台控制命令
    pub fn ptz(sn: u32, device_id: DeviceId, ptz_cmd: impl Into<String>) -> Self {
        Self {
            cmd_type: CmdType::DeviceControl,
            sn,
            device_id,
            ptz_cmd: Some(ptz_cmd.into()),
            tele_boot: None,
            play_time: None,
            alarm_method: None,
        }
    }

    /// 使用方向和速度创建云台控制命令
    pub fn ptz_with_direction(
        sn: u32,
        device_id: DeviceId,
        direction: PtzDirection,
        h_speed: PtzSpeed,
        v_speed: PtzSpeed,
    ) -> Self {
        let cmd = build_ptz_cmd(direction, h_speed, v_speed);
        Self::ptz(sn, device_id, cmd)
    }

    /// 创建停止云台控制命令
    pub fn ptz_stop(sn: u32, device_id: DeviceId) -> Self {
        Self::ptz_with_direction(
            sn,
            device_id,
            PtzDirection::Stop,
            PtzSpeed::new(0),
            PtzSpeed::new(0),
        )
    }

    /// 创建远程启动控制命令
    pub fn remote_start(sn: u32, device_id: DeviceId) -> Self {
        Self {
            cmd_type: CmdType::DeviceControl,
            sn,
            device_id,
            ptz_cmd: None,
            tele_boot: Some("Boot".to_string()),
            play_time: None,
            alarm_method: None,
        }
    }

    /// 创建录像拖动控制命令（历史回放定位）
    ///
    /// `play_time` 格式为 `2024-01-01T00:00:00`。
    pub fn drag_playback(sn: u32, device_id: DeviceId, play_time: impl Into<String>) -> Self {
        Self {
            cmd_type: CmdType::DeviceControl,
            sn,
            device_id,
            ptz_cmd: None,
            tele_boot: None,
            play_time: Some(play_time.into()),
            alarm_method: None,
        }
    }

    /// 创建报警命令
    pub fn alarm_cmd(sn: u32, device_id: DeviceId, alarm_method: impl Into<String>) -> Self {
        Self {
            cmd_type: CmdType::AlarmCmd,
            sn,
            device_id,
            ptz_cmd: None,
            tele_boot: None,
            play_time: None,
            alarm_method: Some(alarm_method.into()),
        }
    }

    /// 创建语音广播命令
    pub fn broadcast(sn: u32, device_id: DeviceId) -> Self {
        Self {
            cmd_type: CmdType::Broadcast,
            sn,
            device_id,
            ptz_cmd: None,
            tele_boot: None,
            play_time: None,
            alarm_method: None,
        }
    }

    /// 将控制消息序列化为 XML 字符串
    pub fn to_xml(&self) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Control>\n  <CmdType>{}</CmdType>\n  <SN>{}</SN>\n  <DeviceID>{}</DeviceID>",
            self.cmd_type.as_str(),
            self.sn,
            self.device_id,
        );

        if let Some(ref ptz_cmd) = self.ptz_cmd {
            xml.push_str(&format!("\n  <PTZCmd>{ptz_cmd}</PTZCmd>"));
        }
        if let Some(ref tele_boot) = self.tele_boot {
            xml.push_str(&format!(
                "\n  <TeleBoot>{}</TeleBoot>",
                escape_xml(tele_boot)
            ));
        }
        if let Some(ref play_time) = self.play_time {
            xml.push_str(&format!(
                "\n  <PlayTime>{}</PlayTime>",
                escape_xml(play_time)
            ));
        }
        if let Some(ref alarm_method) = self.alarm_method {
            xml.push_str(&format!(
                "\n  <AlarmMethod>{}</AlarmMethod>",
                escape_xml(alarm_method)
            ));
        }

        xml.push_str("\n</Control>");
        xml
    }

    /// 从 XML 字符串解析控制消息
    pub fn from_xml(xml: &str) -> Result<Self, XmlError> {
        let body = strip_xml_declaration(xml);

        // 验证根元素
        let inner = extract_between_tags(body, "Control")
            .ok_or_else(|| XmlError::InvalidFormat("missing <Control> root element".to_string()))?;

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

        let ptz_cmd = extract_tag_value(&inner, "PTZCmd");
        let tele_boot = extract_tag_value(&inner, "TeleBoot").map(|v| unescape_xml(&v));
        let play_time = extract_tag_value(&inner, "PlayTime").map(|v| unescape_xml(&v));
        let alarm_method = extract_tag_value(&inner, "AlarmMethod").map(|v| unescape_xml(&v));

        Ok(Self {
            cmd_type,
            sn,
            device_id,
            ptz_cmd,
            tele_boot,
            play_time,
            alarm_method,
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
    fn test_ptz_control_to_xml() {
        let device_id = make_device_id("34020000001320000001");
        let control = Control::ptz(3, device_id, "A50F01001F0000");
        let xml = control.to_xml();

        assert!(xml.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<Control>"));
        assert!(xml.contains("<CmdType>DeviceControl</CmdType>"));
        assert!(xml.contains("<SN>3</SN>"));
        assert!(xml.contains("<DeviceID>34020000001320000001</DeviceID>"));
        assert!(xml.contains("<PTZCmd>A50F01001F0000</PTZCmd>"));
        assert!(xml.contains("</Control>"));
    }

    #[test]
    fn test_ptz_control_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Control>
  <CmdType>DeviceControl</CmdType>
  <SN>3</SN>
  <DeviceID>34020000001320000001</DeviceID>
  <PTZCmd>A50F01001F0000</PTZCmd>
</Control>"#;

        let control = Control::from_xml(xml).unwrap();
        assert_eq!(control.cmd_type, CmdType::DeviceControl);
        assert_eq!(control.sn, 3);
        assert_eq!(control.device_id.as_str(), "34020000001320000001");
        assert_eq!(control.ptz_cmd.as_deref(), Some("A50F01001F0000"));
    }

    #[test]
    fn test_control_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let original = Control::ptz(3, device_id, "A50F01001F0000");
        let xml = original.to_xml();
        let parsed = Control::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, original.cmd_type);
        assert_eq!(parsed.sn, original.sn);
        assert_eq!(parsed.device_id, original.device_id);
        assert_eq!(parsed.ptz_cmd, original.ptz_cmd);
    }

    #[test]
    fn test_control_roundtrip_no_ptz() {
        let device_id = make_device_id("34020000001320000001");
        let original = Control {
            cmd_type: CmdType::DeviceControl,
            sn: 5,
            device_id,
            ptz_cmd: None,
            tele_boot: None,
            play_time: None,
            alarm_method: None,
        };
        let xml = original.to_xml();
        let parsed = Control::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, original.cmd_type);
        assert_eq!(parsed.sn, original.sn);
        assert_eq!(parsed.device_id, original.device_id);
        assert_eq!(parsed.ptz_cmd, None);
    }

    #[test]
    fn test_build_ptz_cmd_stop() {
        let cmd = build_ptz_cmd(PtzDirection::Stop, PtzSpeed::new(0), PtzSpeed::new(0));
        // A5 ^ 00 ^ 00 ^ 00 ^ 00 ^ 00 ^ 00 = A5
        assert_eq!(cmd, "A5000000000000A5");
    }

    #[test]
    fn test_build_ptz_cmd_up() {
        let cmd = build_ptz_cmd(PtzDirection::Up, PtzSpeed::default(), PtzSpeed::default());
        // byte0=A5, byte1=20, byte2=1F, byte3=1F, byte4-6=00
        // checksum = A5 ^ 20 ^ 1F ^ 1F ^ 00 ^ 00 ^ 00 = A5 ^ 20 = 85
        assert_eq!(cmd, "A5201F1F00000085");
    }

    #[test]
    fn test_ptz_with_direction() {
        let device_id = make_device_id("34020000001320000001");
        let control = Control::ptz_with_direction(
            10,
            device_id,
            PtzDirection::Right,
            PtzSpeed::new(0x10),
            PtzSpeed::new(0x10),
        );
        assert!(control.ptz_cmd.is_some());
        let cmd = control.ptz_cmd.unwrap();
        assert!(cmd.starts_with("A5"));
        assert_eq!(cmd.len(), 16);
    }

    #[test]
    fn test_ptz_stop_convenience() {
        let device_id = make_device_id("34020000001320000001");
        let control = Control::ptz_stop(1, device_id);
        assert_eq!(control.cmd_type, CmdType::DeviceControl);
        assert_eq!(control.sn, 1);
        assert!(control.ptz_cmd.is_some());
    }

    #[test]
    fn test_remote_start_to_xml() {
        let device_id = make_device_id("34020000001320000001");
        let control = Control::remote_start(10, device_id);
        let xml = control.to_xml();

        assert!(xml.contains("<CmdType>DeviceControl</CmdType>"));
        assert!(xml.contains("<SN>10</SN>"));
        assert!(xml.contains("<TeleBoot>Boot</TeleBoot>"));
        assert!(!xml.contains("<PTZCmd>"));
    }

    #[test]
    fn test_remote_start_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Control>
  <CmdType>DeviceControl</CmdType>
  <SN>10</SN>
  <DeviceID>34020000001320000001</DeviceID>
  <TeleBoot>Boot</TeleBoot>
</Control>"#;

        let control = Control::from_xml(xml).unwrap();
        assert_eq!(control.cmd_type, CmdType::DeviceControl);
        assert_eq!(control.sn, 10);
        assert_eq!(control.tele_boot.as_deref(), Some("Boot"));
        assert!(control.ptz_cmd.is_none());
    }

    #[test]
    fn test_remote_start_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let original = Control::remote_start(10, device_id);
        let xml = original.to_xml();
        let parsed = Control::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::DeviceControl);
        assert_eq!(parsed.tele_boot.as_deref(), Some("Boot"));
        assert!(parsed.ptz_cmd.is_none());
        assert!(parsed.play_time.is_none());
        assert!(parsed.alarm_method.is_none());
    }

    #[test]
    fn test_drag_playback_to_xml() {
        let device_id = make_device_id("34020000001320000001");
        let control = Control::drag_playback(15, device_id, "2024-01-15T10:30:00");
        let xml = control.to_xml();

        assert!(xml.contains("<CmdType>DeviceControl</CmdType>"));
        assert!(xml.contains("<SN>15</SN>"));
        assert!(xml.contains("<PlayTime>2024-01-15T10:30:00</PlayTime>"));
        assert!(!xml.contains("<PTZCmd>"));
    }

    #[test]
    fn test_drag_playback_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Control>
  <CmdType>DeviceControl</CmdType>
  <SN>15</SN>
  <DeviceID>34020000001320000001</DeviceID>
  <PlayTime>2024-01-15T10:30:00</PlayTime>
</Control>"#;

        let control = Control::from_xml(xml).unwrap();
        assert_eq!(control.cmd_type, CmdType::DeviceControl);
        assert_eq!(control.sn, 15);
        assert_eq!(control.play_time.as_deref(), Some("2024-01-15T10:30:00"));
    }

    #[test]
    fn test_drag_playback_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let original = Control::drag_playback(15, device_id, "2024-01-15T10:30:00");
        let xml = original.to_xml();
        let parsed = Control::from_xml(&xml).unwrap();

        assert_eq!(parsed.play_time.as_deref(), Some("2024-01-15T10:30:00"));
        assert!(parsed.ptz_cmd.is_none());
        assert!(parsed.tele_boot.is_none());
    }

    #[test]
    fn test_alarm_cmd_to_xml() {
        let device_id = make_device_id("34020000001320000001");
        let control = Control::alarm_cmd(20, device_id, "1");
        let xml = control.to_xml();

        assert!(xml.contains("<CmdType>AlarmCmd</CmdType>"));
        assert!(xml.contains("<SN>20</SN>"));
        assert!(xml.contains("<AlarmMethod>1</AlarmMethod>"));
    }

    #[test]
    fn test_alarm_cmd_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Control>
  <CmdType>AlarmCmd</CmdType>
  <SN>20</SN>
  <DeviceID>34020000001320000001</DeviceID>
  <AlarmMethod>1</AlarmMethod>
</Control>"#;

        let control = Control::from_xml(xml).unwrap();
        assert_eq!(control.cmd_type, CmdType::AlarmCmd);
        assert_eq!(control.sn, 20);
        assert_eq!(control.alarm_method.as_deref(), Some("1"));
    }

    #[test]
    fn test_alarm_cmd_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let original = Control::alarm_cmd(20, device_id, "2");
        let xml = original.to_xml();
        let parsed = Control::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::AlarmCmd);
        assert_eq!(parsed.alarm_method.as_deref(), Some("2"));
    }

    #[test]
    fn test_broadcast_to_xml() {
        let device_id = make_device_id("34020000001320000001");
        let control = Control::broadcast(25, device_id);
        let xml = control.to_xml();

        assert!(xml.contains("<CmdType>Broadcast</CmdType>"));
        assert!(xml.contains("<SN>25</SN>"));
        assert!(!xml.contains("<PTZCmd>"));
        assert!(!xml.contains("<TeleBoot>"));
        assert!(!xml.contains("<PlayTime>"));
        assert!(!xml.contains("<AlarmMethod>"));
    }

    #[test]
    fn test_broadcast_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Control>
  <CmdType>Broadcast</CmdType>
  <SN>25</SN>
  <DeviceID>34020000001320000001</DeviceID>
</Control>"#;

        let control = Control::from_xml(xml).unwrap();
        assert_eq!(control.cmd_type, CmdType::Broadcast);
        assert_eq!(control.sn, 25);
        assert!(control.ptz_cmd.is_none());
        assert!(control.tele_boot.is_none());
        assert!(control.play_time.is_none());
        assert!(control.alarm_method.is_none());
    }

    #[test]
    fn test_broadcast_roundtrip() {
        let device_id = make_device_id("34020000001320000001");
        let original = Control::broadcast(25, device_id);
        let xml = original.to_xml();
        let parsed = Control::from_xml(&xml).unwrap();

        assert_eq!(parsed.cmd_type, CmdType::Broadcast);
        assert_eq!(parsed.sn, 25);
        assert_eq!(parsed.device_id, original.device_id);
    }

    #[test]
    fn test_control_from_xml_missing_root() {
        let xml = "<Other><CmdType>DeviceControl</CmdType></Other>";
        let result = Control::from_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_control_from_xml_missing_cmd_type() {
        let xml = "<Control><SN>1</SN><DeviceID>34020000001320000001</DeviceID></Control>";
        let result = Control::from_xml(xml);
        assert!(result.is_err());
    }
}
