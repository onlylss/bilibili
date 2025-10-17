use thiserror::Error;

#[derive(Error, Debug)]
pub enum BiliError {
    #[error("risk control occurred")]
    RiskControlOccurred,
    #[error("risk control verification required, v_voucher: {0}")]
    RiskControlVerificationRequired(String),
    #[error("request failed, status code: {0}, message: {1}")]
    RequestFailed(i64, String),
    #[allow(dead_code)]
    #[error("network timeout or DNS resolution failed")]
    NetworkTimeout,
    #[allow(dead_code)]
    #[error("video stream access denied, code: {0}")]
    VideoStreamDenied(i64),
    #[error("video stream empty: {0}")]
    VideoStreamEmpty(String),
}

impl BiliError {
    /// 根据错误码创建相应的错误类型
    #[allow(dead_code)]
    pub fn from_code_and_message(code: i64, message: String) -> Self {
        match code {
            // 常见的风控相关错误码
            -352 | -412 => Self::RiskControlOccurred,
            // 视频流访问被拒绝
            -404 => Self::VideoStreamDenied(code),
            // 其他错误（充电专享视频现在通过upower字段在获取详情时处理）
            _ => Self::RequestFailed(code, message),
        }
    }

    /// 判断是否为可重试的错误
    #[allow(dead_code)]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::NetworkTimeout => true,
            Self::RiskControlOccurred => false,                // 风控不建议立即重试
            Self::RiskControlVerificationRequired(_) => false, // 需要验证，不建议重试
            Self::VideoStreamDenied(_) => false,
            Self::VideoStreamEmpty(_) => false, // 视频流为空通常不建议重试
            Self::RequestFailed(code, _) => {
                // 网络相关错误码可重试（充电专享视频现在通过upower字段处理）
                matches!(*code, -500..=-400 | -1)
            }
        }
    }

    /// 获取推荐的等待时间（秒）
    #[allow(dead_code)]
    pub fn get_retry_delay(&self) -> Option<u64> {
        match self {
            Self::NetworkTimeout => Some(10),
            Self::RiskControlOccurred => Some(300),           // 风控建议等待5分钟
            Self::RiskControlVerificationRequired(_) => None, // 需要人工验证，不建议自动重试
            Self::RequestFailed(code, _) => match *code {
                -500..=-400 => Some(30), // 服务器错误等待30秒
                _ => None,
            },
            Self::VideoStreamDenied(_) => None,
            Self::VideoStreamEmpty(_) => None,
        }
    }
}
