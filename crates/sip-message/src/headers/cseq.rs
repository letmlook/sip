//! CSeq 头部类型定义与解析
//!
//! CSeq（命令序列号）头部用于标识和排序事务，由序号和方法组成。

use std::fmt;
use std::str::FromStr;

use sip_core::{CSeqNumber, ParseError};

use crate::types::Method;

// ============================================================================
// CSeqHeader - CSeq 头部
// ============================================================================

/// CSeq 头部
///
/// 格式：`CSeq: sequence-number method`
///
/// CSeq 头部用于标识和排序事务。同一个对话中的请求 CSeq 序号必须递增。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CSeqHeader {
    /// 序列号
    pub sequence: CSeqNumber,
    /// 方法
    pub method: Method,
}

impl CSeqHeader {
    /// 创建新的 CSeq 头部
    pub fn new(sequence: u32, method: Method) -> Self {
        Self {
            sequence: CSeqNumber(sequence),
            method,
        }
    }

    /// 递增序列号
    pub fn increment(&mut self) {
        self.sequence.0 = self.sequence.0.saturating_add(1);
    }
}

impl fmt::Display for CSeqHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.sequence.0, self.method)
    }
}

impl FromStr for CSeqHeader {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        // 格式：sequence-number SP method
        let space_pos = s.find(' ').ok_or_else(|| ParseError::InvalidHeader {
            name: "CSeq".to_string(),
            detail: "missing space between sequence and method".to_string(),
        })?;

        let seq_str = &s[..space_pos];
        let method_str = &s[space_pos + 1..];

        let sequence = seq_str
            .parse::<u32>()
            .map_err(|_| ParseError::InvalidHeader {
                name: "CSeq".to_string(),
                detail: format!("invalid sequence number: {}", seq_str),
            })?;

        let method = method_str.trim().parse()?;

        Ok(Self {
            sequence: CSeqNumber(sequence),
            method,
        })
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cseq_header_new() {
        let cseq = CSeqHeader::new(1, Method::Invite);
        assert_eq!(cseq.sequence.0, 1);
        assert_eq!(cseq.method, Method::Invite);
    }

    #[test]
    fn test_cseq_header_display() {
        let cseq = CSeqHeader::new(314159, Method::Invite);
        assert_eq!(cseq.to_string(), "314159 INVITE");
    }

    #[test]
    fn test_cseq_header_parse() {
        let cseq: CSeqHeader = "314159 INVITE".parse().unwrap();
        assert_eq!(cseq.sequence.0, 314159);
        assert_eq!(cseq.method, Method::Invite);
    }

    #[test]
    fn test_cseq_header_parse_register() {
        let cseq: CSeqHeader = "1 REGISTER".parse().unwrap();
        assert_eq!(cseq.sequence.0, 1);
        assert_eq!(cseq.method, Method::Register);
    }

    #[test]
    fn test_cseq_header_increment() {
        let mut cseq = CSeqHeader::new(1, Method::Invite);
        cseq.increment();
        assert_eq!(cseq.sequence.0, 2);
    }

    #[test]
    fn test_cseq_header_roundtrip() {
        let cseq = CSeqHeader::new(42, Method::Bye);
        let s = cseq.to_string();
        let cseq2: CSeqHeader = s.parse().unwrap();
        assert_eq!(cseq.sequence, cseq2.sequence);
        assert_eq!(cseq.method, cseq2.method);
    }

    #[test]
    fn test_cseq_header_parse_invalid() {
        assert!("invalid INVITE".parse::<CSeqHeader>().is_err());
        assert!("1INVITE".parse::<CSeqHeader>().is_err());
    }
}
