//! # gb28181-codec
//!
//! GB28181 20 位国标编码的解析、验证与生成库。
//!
//! ## 编码格式
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
//!
//! ## 快速开始
//!
//! ```
//! use gb28181_codec::{DeviceId, DeviceType, Industry, Province};
//!
//! // 解析国标编码
//! let id = DeviceId::parse("34020000001320000001")?;
//!
//! // 解码为结构化数据
//! let parsed = id.decode()?;
//! println!("省份: {} ({})", parsed.province.name(), parsed.province.code());
//! println!("设备类型: {}", parsed.device_type.name());
//!
//! // 生成国标编码
//! let id = DeviceId::compose(
//!     Province::Anhui,
//!     2,
//!     Industry::SocialSecurity,
//!     DeviceType::FixedCamera,
//!     0,
//!     1,
//! )?;
//!
//! // 构建 SIP URI
//! let uri = id.to_sip_uri("192.168.1.1", 5060);
//! assert_eq!(uri, "sip:34020200113000000001@192.168.1.1:5060");
//!
//! # Ok::<(), gb28181_codec::CodecError>(())
//! ```

mod encoding;
mod types;

pub use types::{CodecError, DeviceId, DeviceType, Industry, ParsedDeviceId, Province};
