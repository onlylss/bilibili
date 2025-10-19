use chrono::Datelike;
use html_escape::decode_html_entities;
use serde_json::json;

use crate::config;

pub fn video_format_args(video_model: &bili_sync_entity::video::Model) -> serde_json::Value {
    let current_config = config::reload_config();
    // 解码HTML实体，确保UP主名称正确显示
    let decoded_upper_name = decode_html_entities(&video_model.upper_name).to_string();

    json!({
        "bvid": &video_model.bvid,
        "title": &video_model.name,
        "upper_name": decoded_upper_name,
        "upper_mid": &video_model.upper_id,
        "pubtime": &video_model.pubtime.and_utc().format(&current_config.time_format).to_string(),
        "fav_time": &video_model.favtime.and_utc().format(&current_config.time_format).to_string(),
        "show_title": &video_model.name,
    })
}

pub fn page_format_args(
    video_model: &bili_sync_entity::video::Model,
    page_model: &bili_sync_entity::page::Model,
) -> serde_json::Value {
    let current_config = config::reload_config();

    // 检查是否为单P视频
    let is_single_page = video_model.single_page.unwrap_or(true);

    if !is_single_page {
        // 对于多P视频（非番剧），使用番剧格式的命名，默认季度为1
        let season_number = 1;

        // 从发布时间提取年份
        let year = video_model.pubtime.year();

        // 生成分辨率信息
        let resolution = match (page_model.width, page_model.height) {
            (Some(w), Some(h)) => format!("{}x{}", w, h),
            _ => "Unknown".to_string(),
        };

        // 解码HTML实体，确保UP主名称正确显示
        let decoded_upper_name = decode_html_entities(&video_model.upper_name).to_string();

        json!({
            "bvid": &video_model.bvid,
            "title": &video_model.name,
            "upper_name": &decoded_upper_name,
            "upper_mid": &video_model.upper_id,
            "ptitle": &page_model.name,
            "pid": page_model.pid,
            "pid_pad": format!("{:02}", page_model.pid),
            "season": season_number,
            "season_pad": format!("{:02}", season_number),
            "year": year,
            "studio": &decoded_upper_name,
            "category": video_model.category,
            "resolution": resolution,
            "pubtime": video_model.pubtime.and_utc().format(&current_config.time_format).to_string(),
            "fav_time": video_model.favtime.and_utc().format(&current_config.time_format).to_string(),
            "long_title": &page_model.name,
            "show_title": &page_model.name,
        })
    } else {
        // 对于单P视频，使用原有的格式（不包含season_pad）
        // 解码HTML实体，确保UP主名称正确显示
        let decoded_upper_name = decode_html_entities(&video_model.upper_name).to_string();

        json!({
            "bvid": &video_model.bvid,
            "title": &video_model.name,
            "upper_name": &decoded_upper_name,
            "upper_mid": &video_model.upper_id,
            "ptitle": &page_model.name,
            "pid": page_model.pid,
            "pid_pad": format!("{:02}", page_model.pid),
            "pubtime": video_model.pubtime.and_utc().format(&current_config.time_format).to_string(),
            "fav_time": video_model.favtime.and_utc().format(&current_config.time_format).to_string(),
            "long_title": &page_model.name,
            "show_title": &page_model.name,
        })
    }
}
