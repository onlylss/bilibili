use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::config::NotificationConfig;

// Server酱API请求结构
#[derive(Serialize)]
struct ServerChanRequest {
    title: String,
    desp: String,
}

// Server酱API响应结构
#[derive(Deserialize)]
struct ServerChanResponse {
    #[serde(deserialize_with = "deserialize_code")]
    code: i32,
    message: String,
    #[serde(default)]
    #[allow(dead_code)]
    data: Option<serde_json::Value>,
}

// 自定义反序列化器，支持字符串和整数的code
fn deserialize_code<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;

    match value {
        serde_json::Value::Number(n) => n
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(|| D::Error::custom("code is not a valid i32")),
        serde_json::Value::String(s) => s
            .parse::<i32>()
            .map_err(|_| D::Error::custom(format!("code string '{}' is not a valid i32", s))),
        _ => Err(D::Error::custom("code must be a number or string")),
    }
}

// 推送通知客户端
pub struct NotificationClient {
    client: Client,
    config: NotificationConfig,
}

// 扫描结果数据结构
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NewVideoInfo {
    pub title: String,
    pub bvid: String,
    pub upper_name: String,
    pub source_type: String,
    pub source_name: String,
    pub pubtime: Option<String>, // 使用字符串格式的北京时间
    pub episode_number: Option<i32>,
    pub season_number: Option<i32>,
    pub video_id: Option<i32>, // 添加视频ID字段，用于过滤删除队列中的视频
}

#[derive(Debug, Clone)]
pub struct SourceScanResult {
    pub source_type: String,
    pub source_name: String,
    pub new_videos: Vec<NewVideoInfo>,
}

#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub total_sources: usize,
    pub total_new_videos: usize,
    pub scan_duration: Duration,
    pub source_results: Vec<SourceScanResult>,
}

impl NotificationClient {
    pub fn new(config: NotificationConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.notification_timeout))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    // 清理可能导致Server酱数据库问题的特殊字符
    fn sanitize_for_serverchan(text: &str) -> String {
        text
            .replace('「', "[")
            .replace('」', "]")
            .replace('【', "[")
            .replace('】', "]")
            .replace('〖', "[")
            .replace('〗', "]")
            .replace('〔', "[")
            .replace('〕', "]")
            // 移除其他可能有问题的Unicode字符
            .chars()
            .filter(|c| c.is_ascii() || (*c as u32) < 0x10000)
            .collect()
    }

    pub async fn send_scan_completion(&self, summary: &ScanSummary) -> Result<()> {
        if !self.config.enable_scan_notifications {
            debug!("推送通知已禁用，跳过发送");
            return Ok(());
        }

        if summary.total_new_videos < self.config.notification_min_videos {
            debug!(
                "新增视频数量({})未达到推送阈值({})",
                summary.total_new_videos, self.config.notification_min_videos
            );
            return Ok(());
        }

        let Some(ref key) = self.config.serverchan_key else {
            warn!("未配置Server酱密钥，无法发送推送");
            return Ok(());
        };

        let (title, content) = self.format_scan_message(summary);

        for attempt in 1..=self.config.notification_retry_count {
            match self.send_to_serverchan(key, &title, &content).await {
                Ok(_) => {
                    info!("扫描完成推送发送成功");
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        "推送发送失败 (尝试 {}/{}): {}",
                        attempt, self.config.notification_retry_count, e
                    );

                    if attempt < self.config.notification_retry_count {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }

        error!("推送发送失败，已达到最大重试次数");
        Ok(()) // 不返回错误，避免影响主要功能
    }

    async fn send_to_serverchan(&self, key: &str, title: &str, content: &str) -> Result<()> {
        let url = format!("https://sctapi.ftqq.com/{}.send", key);
        let request = ServerChanRequest {
            title: title.to_string(),
            desp: content.to_string(),
        };

        let response = self.client.post(&url).json(&request).send().await?;

        let response_text = response.text().await?;
        let server_response: ServerChanResponse = serde_json::from_str(&response_text)
            .map_err(|e| anyhow!("解析响应失败: {}, 响应内容: {}", e, response_text))?;

        if server_response.code == 0 {
            Ok(())
        } else {
            Err(anyhow!("Server酱返回错误: {}", server_response.message))
        }
    }

    fn format_scan_message(&self, summary: &ScanSummary) -> (String, String) {
        let title = "Bili Sync 扫描完成".to_string();

        // 限制最大内容长度为30KB（留一些余量）
        const MAX_CONTENT_LENGTH: usize = 30000;

        let mut content = format!(
            "📊 **扫描摘要**\n\n- 扫描视频源: {}个\n- 新增视频: {}个\n- 扫描耗时: {:.1}分钟\n\n",
            summary.total_sources,
            summary.total_new_videos,
            summary.scan_duration.as_secs_f64() / 60.0
        );

        if summary.total_new_videos > 0 {
            content.push_str("📹 **新增视频详情**\n\n");

            let mut videos_shown = 0;
            let mut sources_shown = 0;

            for source_result in &summary.source_results {
                if !source_result.new_videos.is_empty() {
                    // 如果内容已经很长，停止添加更多内容
                    if content.len() > MAX_CONTENT_LENGTH - 500 {
                        let remaining_videos = summary.total_new_videos - videos_shown;
                        let remaining_sources = summary
                            .source_results
                            .iter()
                            .filter(|s| !s.new_videos.is_empty())
                            .count()
                            - sources_shown;
                        content.push_str(&format!(
                            "\n...还有 {} 个视频源的 {} 个新视频（内容过长已省略）\n",
                            remaining_sources, remaining_videos
                        ));
                        break;
                    }

                    sources_shown += 1;

                    let icon = match source_result.source_type.as_str() {
                        "收藏夹" => "🎬",
                        "合集" => "📁",
                        "UP主投稿" => "🎯",
                        "稍后再看" => "⏰",
                        "番剧" => "📺",
                        _ => "📄",
                    };

                    // 清理源名称中的特殊字符
                    let clean_source_name = Self::sanitize_for_serverchan(&source_result.source_name);

                    content.push_str(&format!(
                        "{} **{}** - {} ({}个新视频):\n",
                        icon,
                        source_result.source_type,
                        clean_source_name,
                        source_result.new_videos.len()
                    ));

                    // 按照视频类型进行排序
                    let mut sorted_videos = source_result.new_videos.clone();
                    if source_result.source_type == "番剧" {
                        // 番剧按集数降序排列（最新的集数在前）
                        sorted_videos.sort_by(|a, b| b.episode_number.unwrap_or(0).cmp(&a.episode_number.unwrap_or(0)));
                    } else {
                        // 其他视频按发布时间降序排列（最新的在前）
                        sorted_videos.sort_by(|a, b| {
                            b.pubtime
                                .as_ref()
                                .unwrap_or(&String::new())
                                .cmp(a.pubtime.as_ref().unwrap_or(&String::new()))
                        });
                    }

                    // 限制每个源显示的视频数量
                    let max_videos_per_source = 20;
                    let videos_to_show = sorted_videos.len().min(max_videos_per_source);

                    for (idx, video) in sorted_videos.iter().take(videos_to_show).enumerate() {
                        // 如果内容过长，提前结束
                        if content.len() > MAX_CONTENT_LENGTH - 1000 {
                            content.push_str(&format!(
                                "...还有 {} 个视频（内容过长已省略）\n",
                                sorted_videos.len() - idx
                            ));
                            break;
                        }

                        videos_shown += 1;

                        // 清理视频标题中的特殊字符
                        let clean_title = Self::sanitize_for_serverchan(&video.title);
                        let mut video_line =
                            format!("- [{}](https://www.bilibili.com/video/{})", clean_title, video.bvid);

                        // 添加额外信息
                        if source_result.source_type == "番剧" && video.episode_number.is_some() {
                            video_line.push_str(&format!(" (第{}集", video.episode_number.unwrap()));
                            // 番剧也显示时间戳
                            if let Some(pubtime) = &video.pubtime {
                                // 只显示日期部分，不显示时间
                                if let Some(date_part) = pubtime.split(' ').next() {
                                    video_line.push_str(&format!(", {}", date_part));
                                }
                            }
                            video_line.push(')');
                        } else if let Some(pubtime) = &video.pubtime {
                            // 只显示日期部分，不显示时间
                            if let Some(date_part) = pubtime.split(' ').next() {
                                video_line.push_str(&format!(" ({})", date_part));
                            }
                        }

                        content.push_str(&video_line);
                        content.push('\n');
                    }

                    // 如果有未显示的视频，添加提示
                    if sorted_videos.len() > videos_to_show {
                        content.push_str(&format!("...还有 {} 个视频\n", sorted_videos.len() - videos_to_show));
                    }

                    content.push('\n');
                }
            }
        }

        // 最终清理整个内容，确保没有问题字符
        let clean_content = Self::sanitize_for_serverchan(&content);

        // 确保内容不超过限制
        let final_content = if clean_content.len() > MAX_CONTENT_LENGTH {
            let mut truncated = clean_content.chars().take(MAX_CONTENT_LENGTH - 100).collect::<String>();
            truncated.push_str("\n\n...内容过长，已截断");
            truncated
        } else {
            clean_content
        };

        (title, final_content)
    }

    pub async fn test_notification(&self) -> Result<()> {
        let Some(ref key) = self.config.serverchan_key else {
            return Err(anyhow!("未配置Server酱密钥"));
        };

        let title = "Bili Sync 测试推送";
        let content = "这是一条测试推送消息，如果您收到此消息，说明推送配置正确。\n\n🎉 推送功能工作正常！";

        self.send_to_serverchan(key, title, content).await
    }

    pub async fn send_custom_test(&self, message: &str) -> Result<()> {
        let Some(ref key) = self.config.serverchan_key else {
            return Err(anyhow!("未配置Server酱密钥"));
        };

        let title = "Bili Sync 自定义测试推送";
        let content = format!("🧪 **自定义测试消息**\n\n{}", message);

        self.send_to_serverchan(key, title, &content).await
    }
}

// 便捷函数
pub async fn send_scan_notification(summary: ScanSummary) -> Result<()> {
    let config = crate::config::reload_config().notification;
    let client = NotificationClient::new(config);
    client.send_scan_completion(&summary).await
}

#[allow(dead_code)]
pub async fn test_notification() -> Result<()> {
    let config = crate::config::reload_config().notification;
    let client = NotificationClient::new(config);
    client.test_notification().await
}

#[allow(dead_code)]
pub async fn send_custom_test_notification(message: &str) -> Result<()> {
    let config = crate::config::reload_config().notification;
    let client = NotificationClient::new(config);
    client.send_custom_test(message).await
}
