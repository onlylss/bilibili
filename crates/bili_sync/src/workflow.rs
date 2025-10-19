use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Result};
use bili_sync_entity::*;
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt, TryStreamExt};
use sea_orm::entity::prelude::*;
use sea_orm::ActiveValue::Set;
use sea_orm::{DatabaseBackend, Statement, TransactionTrait};
use tokio::fs;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::utils::time_format::now_standard_string;

use crate::adapter::{video_source_from, Args, VideoSource, VideoSourceEnum};
use crate::bilibili::{
    BestStream, BiliClient, BiliError, Dimension, PageInfo, Stream as VideoStream, Video, VideoInfo,
};
use crate::config::ARGS;
use crate::error::{DownloadAbortError, ExecutionStatus, ProcessPageError};
use crate::task::{DeleteVideoTask, VIDEO_DELETE_TASK_QUEUE};
use crate::unified_downloader::UnifiedDownloader;
use crate::utils::format_arg::{page_format_args, video_format_args};
use crate::utils::model::{
    create_pages, create_videos, filter_unfilled_videos, filter_unhandled_video_pages,
    get_failed_videos_in_current_cycle, update_pages_model, update_videos_model,
};
use crate::utils::nfo::NFO;
use crate::utils::notification::NewVideoInfo;
use crate::utils::status::{PageStatus, VideoStatus, STATUS_OK};

/// 完整地处理某个视频来源，返回新增的视频数量和视频信息
pub async fn process_video_source(
    args: &Args,
    bili_client: &BiliClient,
    path: &Path,
    connection: &DatabaseConnection,
    downloader: &UnifiedDownloader,
    token: CancellationToken,
) -> Result<(usize, Vec<NewVideoInfo>)> {
    // 定义一个辅助函数来处理-101错误并重试
    let retry_with_refresh = |error_msg: String| async move {
        if error_msg.contains("status code: -101") || error_msg.contains("账号未登录") {
            warn!("检测到登录状态过期，尝试刷新凭据...");
            if let Err(refresh_err) = bili_client.check_refresh().await {
                error!("刷新凭据失败：{:#}", refresh_err);
                return Err(refresh_err);
            } else {
                info!("凭据刷新成功，将重试操作");
                return Ok(());
            }
        }
        Err(anyhow::anyhow!("非登录状态错误，无需刷新凭据"))
    };

    // 从参数中获取视频列表的 Model 与视频流
    let (video_source, video_streams) =
        match video_source_from(args, path, bili_client, connection, Some(token.clone())).await {
            Ok(result) => result,
            Err(e) => {
                let error_msg = format!("{:#}", e);
                if retry_with_refresh(error_msg).await.is_ok() {
                    // 刷新成功，重试
                    video_source_from(args, path, bili_client, connection, Some(token.clone())).await?
                } else {
                    return Err(e);
                }
            }
        };

    // 从视频流中获取新视频的简要信息，写入数据库，并获取新增视频数量和信息
    let (new_video_count, new_videos) =
        match refresh_video_source(&video_source, video_streams, connection, token.clone(), bili_client).await {
            Ok(result) => result,
            Err(e) => {
                let error_msg = format!("{:#}", e);
                if retry_with_refresh(error_msg).await.is_ok() {
                    // 刷新成功，重新获取视频流并重试
                    let (_, video_streams) =
                        video_source_from(args, path, bili_client, connection, Some(token.clone())).await?;
                    refresh_video_source(&video_source, video_streams, connection, token.clone(), bili_client).await?
                } else {
                    return Err(e);
                }
            }
        };

    // Guard: skip further steps if paused/cancelled or no new videos in this round
    if crate::task::TASK_CONTROLLER.is_paused() || token.is_cancelled() {
        info!("任务已暂停/取消，跳过详情与下载阶段");
        return Ok((new_video_count, new_videos));
    }
    if new_video_count == 0 {
        let has_unfilled = !filter_unfilled_videos(video_source.filter_expr(), connection).await?.is_empty();
        let has_unhandled = !filter_unhandled_video_pages(video_source.filter_expr(), connection).await?.is_empty();
        let has_failed = !get_failed_videos_in_current_cycle(video_source.filter_expr(), connection).await?.is_empty();

        // 检查是否有弹幕文件需要处理（删除或下载）
        let has_danmaku_work = check_danmaku_work_needed(&video_source, connection).await?;

        if !(has_unfilled || has_unhandled || has_failed || has_danmaku_work) {
            info!("本轮未发现新视频，且无待处理任务，跳过详情与下载阶段");
            return Ok((new_video_count, new_videos));
        } else if has_danmaku_work {
            if video_source.download_danmaku() {
                info!("本轮未发现新视频，但弹幕已开启且有缺失文件，继续执行下载阶段");
            } else {
                info!("本轮未发现新视频，但弹幕已关闭且有文件需清理，继续执行下载阶段");
            }
        } else {
            info!("本轮未发现新视频，但存在待处理任务（重置/未完成/可重试），继续执行下载阶段");
        }
    }

    // 单独请求视频详情接口，获取视频的详情信息与所有的分页，写入数据库
    if let Err(e) = fetch_video_details(bili_client, &video_source, connection, token.clone()).await {
        // 新增：检查是否为风控导致的下载中止
        if e.downcast_ref::<DownloadAbortError>().is_some() {
            error!("获取视频详情时触发风控，已终止当前视频源的处理，停止所有后续扫描");
            // 风控时应该返回错误，中断整个扫描循环，而不是继续处理下一个视频源
            return Err(e);
        }

        let error_msg = format!("{:#}", e);
        if retry_with_refresh(error_msg).await.is_ok() {
            // 刷新成功，重试
            fetch_video_details(bili_client, &video_source, connection, token.clone()).await?;
        } else {
            return Err(e);
        }
    }

    if ARGS.scan_only {
        warn!("已开启仅扫描模式，跳过视频下载..");
    } else {
        // 从数据库中查找所有未下载的视频与分页，下载并处理
        if let Err(e) =
            download_unprocessed_videos(bili_client, &video_source, connection, downloader, token.clone()).await
        {
            let error_msg = format!("{:#}", e);
            if retry_with_refresh(error_msg).await.is_ok() {
                // 刷新成功，重试（继续使用原有的取消令牌）
                download_unprocessed_videos(bili_client, &video_source, connection, downloader, token.clone()).await?;
            } else {
                return Err(e);
            }
        }

        // 新增：循环内重试失败的视频
        // 在当前扫描循环结束前，对失败的视频进行一次额外的重试机会
        if let Err(e) =
            retry_failed_videos_once(bili_client, &video_source, connection, downloader, token.clone()).await
        {
            warn!("循环内重试失败的视频时出错: {:#}", e);
            // 重试失败不中断主流程，继续执行
        }
    }
    Ok((new_video_count, new_videos))
}

/// 请求接口，获取视频列表中所有新添加的视频信息，将其写入数据库
pub async fn refresh_video_source<'a>(
    video_source: &VideoSourceEnum,
    video_streams: Pin<Box<dyn Stream<Item = Result<VideoInfo>> + 'a + Send>>,
    connection: &DatabaseConnection,
    token: CancellationToken,
    bili_client: &BiliClient,
) -> Result<(usize, Vec<NewVideoInfo>)> {
    video_source.log_refresh_video_start();
    let latest_row_at_string = video_source.get_latest_row_at();
    let latest_row_at = crate::utils::time_format::parse_time_string(&latest_row_at_string)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap().naive_utc())
        .and_utc();
    let mut max_datetime = latest_row_at;
    let mut error = Ok(());
    let mut video_streams = video_streams
        .take_while(|res| {
            if token.is_cancelled() {
                return futures::future::ready(false);
            }
            match res {
                Err(e) => {
                    error = Err(anyhow!(e.to_string()));
                    futures::future::ready(false)
                }
                Ok(v) => {
                    // 虽然 video_streams 是从新到旧的，但由于此处是分页请求，极端情况下可能发生访问完第一页时插入了两整页视频的情况
                    // 此时获取到的第二页视频比第一页的还要新，因此为了确保正确，理应对每一页的第一个视频进行时间比较
                    // 但在 streams 的抽象下，无法判断具体是在哪里分页的，所以暂且对每个视频都进行比较，应该不会有太大性能损失
                    let release_datetime = v.release_datetime();
                    if release_datetime > &max_datetime {
                        max_datetime = *release_datetime;
                    }
                    futures::future::ready(video_source.should_take(release_datetime, latest_row_at_string.as_str()))
                }
            }
        })
        .filter_map(|res| futures::future::ready(res.ok()))
        .chunks(10);
    let mut count = 0;
    let mut new_videos = Vec::new();

    while let Some(videos_info) = video_streams.next().await {
        // 在处理每批视频前检查取消状态
        if token.is_cancelled() || crate::task::TASK_CONTROLLER.is_paused() {
            warn!("视频源处理过程中检测到取消/暂停信号，停止处理");
            break;
        }
        // 获取插入前的视频数量
        let before_count = get_video_count_for_source(video_source, connection).await?;

        // 先收集需要的视频信息（包括集数信息和ep_id）
        let mut temp_video_infos = Vec::new();
        for video_info in &videos_info {
            let (title, bvid, upper_name, episode_num, ep_id) = match video_info {
                VideoInfo::Detail { title, bvid, upper, .. } => {
                    (title.clone(), bvid.clone(), upper.name.clone(), None, None)
                }
                VideoInfo::Favorite { title, bvid, upper, .. } => {
                    (title.clone(), bvid.clone(), upper.name.clone(), None, None)
                }
                VideoInfo::Collection { title, bvid, arc, .. } => {
                    // 从arc字段中提取upper信息
                    let upper_name = arc
                        .as_ref()
                        .and_then(|a| a["author"]["name"].as_str())
                        .unwrap_or("未知")
                        .to_string();
                    (title.clone(), bvid.clone(), upper_name, None, None)
                }
                VideoInfo::WatchLater { title, bvid, upper, .. } => {
                    (title.clone(), bvid.clone(), upper.name.clone(), None, None)
                }
                VideoInfo::Submission { title, bvid, .. } => {
                    // Submission 没有 upper 信息，使用默认值
                    (title.clone(), bvid.clone(), "未知".to_string(), None, None)
                }
                VideoInfo::Bangumi {
                    title,
                    bvid,
                    episode_number,
                    ep_id,
                    ..
                } => {
                    // Bangumi 包含 ep_id 信息，用于唯一标识
                    (
                        title.clone(),
                        bvid.clone(),
                        "番剧".to_string(),
                        *episode_number,
                        Some(ep_id.clone()),
                    )
                }
            };
            temp_video_infos.push((title, bvid, upper_name, episode_num, ep_id));
        }

        // 获取所有视频的BVID，用于后续判断哪些是新增的
        let video_bvids: Vec<String> = videos_info
            .iter()
            .map(|v| match v {
                VideoInfo::Detail { bvid, .. } => bvid.clone(),
                VideoInfo::Favorite { bvid, .. } => bvid.clone(),
                VideoInfo::Collection { bvid, .. } => bvid.clone(),
                VideoInfo::WatchLater { bvid, .. } => bvid.clone(),
                VideoInfo::Submission { bvid, .. } => bvid.clone(),
                VideoInfo::Bangumi { bvid, .. } => bvid.clone(),
            })
            .collect();

        create_videos(videos_info, video_source, connection).await?;

        // 获取插入后的视频数量，计算实际新增数量
        let after_count = get_video_count_for_source(video_source, connection).await?;
        let new_count = after_count - before_count;
        count += new_count;

        // 如果有新增视频，通过查询数据库来确定哪些是新增的
        if new_count > 0 {
            // 查询这批视频中哪些是新插入的（根据创建时间）
            let now = crate::utils::time_format::beijing_now();
            let recent_threshold = now - chrono::Duration::seconds(10); // 10秒内创建的视频

            let newly_inserted = video::Entity::find()
                .filter(video_source.filter_expr())
                .filter(video::Column::Bvid.is_in(video_bvids.clone()))
                .filter(video::Column::CreatedAt.gte(recent_threshold.format("%Y-%m-%d %H:%M:%S").to_string()))
                .all(connection)
                .await?;

            debug!("查询到 {} 个新插入的视频记录", newly_inserted.len());

            // 为每个新插入的视频创建通知信息
            for new_video in newly_inserted {
                // 查找对应的视频信息，对番剧使用ep_id进行精确匹配
                let video_info_idx = if new_video.source_type == Some(1) && new_video.ep_id.is_some() {
                    // 番剧：使用ep_id匹配
                    temp_video_infos.iter().position(
                        |(_, _, _, _, ep_id): &(String, String, String, Option<i32>, Option<String>)| {
                            ep_id.as_ref() == new_video.ep_id.as_ref()
                        },
                    )
                } else {
                    // 其他类型：使用bvid匹配
                    temp_video_infos
                        .iter()
                        .position(|(_, bvid, _, _, _)| bvid == &new_video.bvid)
                };

                if let Some(idx) = video_info_idx {
                    let (title, _, upper_name, bangumi_episode, _) = &temp_video_infos[idx];

                    // 使用数据库中的发布时间（已经是北京时间）
                    let pubtime = new_video.pubtime.format("%Y-%m-%d %H:%M:%S").to_string();

                    // 获取集数信息
                    let episode_number = if let Some(ep) = bangumi_episode {
                        // 番剧：使用从VideoInfo中获取的集数
                        Some(*ep)
                    } else {
                        // 其他类型：使用数据库中的episode_number字段
                        new_video.episode_number
                    };

                    new_videos.push(NewVideoInfo {
                        title: title.clone(),
                        bvid: new_video.bvid.clone(),
                        upper_name: upper_name.clone(),
                        source_type: video_source.source_type_display(),
                        source_name: video_source.source_name_display(),
                        pubtime: Some(pubtime),
                        episode_number,
                        season_number: None,
                        video_id: Some(new_video.id), // 添加视频ID，用于后续过滤
                    });
                }
            }

            debug!("实际收集到 {} 个新视频信息用于推送", new_videos.len());
        }
    }
    // 如果获取视频分页过程中发生了错误，直接在此处返回，不更新 latest_row_at
    error?;
    if max_datetime != latest_row_at {
        // 转换为北京时间的标准字符串格式
        let beijing_datetime = max_datetime.with_timezone(&crate::utils::time_format::beijing_timezone());
        let beijing_datetime_string = beijing_datetime.format("%Y-%m-%d %H:%M:%S").to_string();
        video_source
            .update_latest_row_at(beijing_datetime_string)
            .save(connection)
            .await?;
    }


    video_source.log_refresh_video_end(count);
    debug!("workflow返回: count={}, new_videos.len()={}", count, new_videos.len());
    Ok((count, new_videos))
}

/// 筛选出所有未获取到全部信息的视频，尝试补充其详细信息
pub async fn fetch_video_details(
    bili_client: &BiliClient,
    video_source: &VideoSourceEnum,
    connection: &DatabaseConnection,
    token: CancellationToken,
) -> Result<()> {
    // Early exit when paused/cancelled
    if crate::task::TASK_CONTROLLER.is_paused() || token.is_cancelled() {
        info!("任务已暂停/取消，跳过视频详情阶段");
        return Ok(());
    }
    video_source.log_fetch_video_start();
    let videos_model = filter_unfilled_videos(video_source.filter_expr(), connection).await?;

    // 处理视频 - 使用并发处理优化性能
    if !videos_model.is_empty() {
        info!("开始并发处理 {} 个视频的详情", videos_model.len());

        // 使用信号量控制并发数
        let current_config = crate::config::reload_config();
        let semaphore = Semaphore::new(current_config.concurrent_limit.video);

        let tasks = videos_model
            .into_iter()
            .map(|video_model| {
                let semaphore = &semaphore;
                let token = token.clone();
                async move {
                    // 获取许可以控制并发
                    let _permit = tokio::select! {
                        biased;
                        _ = token.cancelled() => return Err(anyhow!("Download cancelled")),
                        permit = semaphore.acquire() => permit.context("acquire semaphore failed")?,
                    };

                    let video = Video::new(bili_client, video_model.bvid.clone());
                    let info: Result<_> = tokio::select! {
                        biased;
                        _ = token.cancelled() => return Err(anyhow!("Download cancelled")),
                        res = async { Ok((video.get_tags().await?, video.get_view_info().await?)) } => res,
                    };
                    match info {
                        Err(e) => {
                            // 新增：检查是否为风控错误
                            let classified_error = crate::error::ErrorClassifier::classify_error(&e);
                            if classified_error.error_type == crate::error::ErrorType::RiskControl {
                                error!(
                                    "获取视频 {} - {} 的详细信息时触发风控: {}",
                                    &video_model.bvid, &video_model.name, classified_error.message
                                );
                                // 返回一个特定的错误来中止整个批处理
                                return Err(anyhow!(DownloadAbortError()));
                            }

                            error!(
                                "获取视频 {} - {} 的详细信息失败，错误为：{:#}",
                                &video_model.bvid, &video_model.name, e
                            );
                            if let Some(BiliError::RequestFailed(-404, _)) = e.downcast_ref::<BiliError>() {
                                let mut video_active_model: bili_sync_entity::video::ActiveModel = video_model.into();
                                video_active_model.valid = Set(false);
                                video_active_model.save(connection).await?;

                                // 触发异步同步到内存DB
                            }
                        }
                        Ok((tags, mut view_info)) => {
                            let VideoInfo::Detail {
                                pages,
                                staff,
                                ref is_upower_exclusive,
                                ref is_upower_play,
                                ..
                            } = &mut view_info
                            else {
                                unreachable!()
                            };

                            // 革命性充电视频检测：基于API返回的upower字段进行精确判断
                            if let (Some(true), Some(false)) = (is_upower_exclusive, is_upower_play) {
                                info!("「{}」检测到充电专享视频（未充电），将自动删除", &video_model.name);
                                // 创建自动删除任务
                                let delete_task = DeleteVideoTask {
                                    video_id: video_model.id,
                                    task_id: format!("auto_delete_upower_{}", video_model.id),
                                };

                                if let Err(delete_err) =
                                    VIDEO_DELETE_TASK_QUEUE.enqueue_task(delete_task, connection).await
                                {
                                    error!("创建充电视频删除任务失败「{}」: {:#}", &video_model.name, delete_err);
                                } else {
                                    debug!("充电视频删除任务已加入队列「{}」", &video_model.name);
                                }

                                // 跳过后续处理，返回成功完成这个视频的处理
                                return Ok(());
                            }

                            // 日志记录upower字段状态（仅debug级别）
                            if is_upower_exclusive.is_some() || is_upower_play.is_some() {
                                debug!(
                                    "视频「{}」upower状态: exclusive={:?}, play={:?}",
                                    &video_model.name, is_upower_exclusive, is_upower_play
                                );
                            }

                            let pages = std::mem::take(pages);
                            let pages_len = pages.len();

                            // 提取第一个page的cid用于更新video表
                            let first_page_cid = pages.first().map(|p| p.cid);

                            // 调试日志：检查staff信息
                            if let Some(staff_list) = staff {
                                debug!("视频 {} 有staff信息，成员数量: {}", video_model.bvid, staff_list.len());
                                for staff_member in staff_list.iter() {
                                    debug!(
                                        "  - staff: mid={}, title={}, name={}",
                                        staff_member.mid, staff_member.title, staff_member.name
                                    );
                                }
                            } else {
                                debug!("视频 {} 没有staff信息", video_model.bvid);
                            }

                            // 检查是否为合作视频，支持submission和收藏夹来源
                            let mut video_model_mut = video_model.clone();
                            let mut collaboration_video_updated = false;
                            if let Some(staff_list) = staff {
                                debug!("视频 {} 有staff信息，成员数量: {}", video_model.bvid, staff_list.len());
                                for staff_member in staff_list.iter() {
                                    debug!(
                                        "  - staff: mid={}, title={}, name={}",
                                        staff_member.mid, staff_member.title, staff_member.name
                                    );
                                }

                                if staff_list.len() > 1 {
                                    debug!(
                                        "发现合作视频：bvid={}, staff_count={}",
                                        video_model.bvid,
                                        staff_list.len()
                                    );

                                    // 查找所有可能的订阅UP主
                                    let mut matched_submission: Option<submission::Model> = None;

                                    // 1. 如果是submission来源，直接使用source_submission_id
                                    if let Some(source_submission_id) = video_model.source_submission_id {
                                        debug!("submission来源视频，source_submission_id: {}", source_submission_id);
                                        if let Ok(Some(submission)) =
                                            submission::Entity::find_by_id(source_submission_id)
                                                .one(connection)
                                                .await
                                        {
                                            debug!(
                                                "找到来源submission: {} ({})",
                                                submission.upper_name, submission.upper_id
                                            );
                                            // 检查这个submission的UP主是否在staff列表中
                                            if staff_list.iter().any(|staff| staff.mid == submission.upper_id) {
                                                debug!(
                                                    "submission UP主 {} ({}) 在staff列表中",
                                                    submission.upper_name, submission.upper_id
                                                );
                                                matched_submission = Some(submission);
                                            } else {
                                                debug!(
                                                    "submission UP主 {} ({}) 不在staff列表中",
                                                    submission.upper_name, submission.upper_id
                                                );
                                            }
                                        } else {
                                            debug!(
                                                "找不到source_submission_id对应的submission记录: {}",
                                                source_submission_id
                                            );
                                        }
                                    } else {
                                        debug!("非submission来源视频，检查staff中是否有已订阅的UP主");
                                        // 2. 如果不是submission来源（如收藏夹），查找所有subscription中匹配的UP主
                                        for staff_member in staff_list.iter() {
                                            debug!(
                                                "检查staff成员 {} ({}) 是否已订阅",
                                                staff_member.name, staff_member.mid
                                            );
                                            if let Ok(Some(submission)) = submission::Entity::find()
                                                .filter(submission::Column::UpperId.eq(staff_member.mid))
                                                .filter(submission::Column::Enabled.eq(true))
                                                .one(connection)
                                                .await
                                            {
                                                debug!(
                                                    "在staff中找到已订阅的UP主：{} ({})",
                                                    staff_member.name, staff_member.mid
                                                );
                                                matched_submission = Some(submission);
                                                break;
                                            } else {
                                                debug!("staff成员 {} ({}) 未订阅", staff_member.name, staff_member.mid);
                                            }
                                        }
                                    }

                                    // 如果找到了匹配的订阅UP主，进行归类
                                    if let Some(submission) = matched_submission {
                                        // 从staff信息中找到匹配UP主的头像
                                        let matched_staff_face = staff_list
                                            .iter()
                                            .find(|staff| staff.mid == submission.upper_id)
                                            .map(|staff| staff.face.clone())
                                            .unwrap_or_default();

                                        debug!(
                                            "为合作视频匹配UP主头像: {} -> {}",
                                            submission.upper_name, matched_staff_face
                                        );

                                        // 使用submission的信息更新视频，包括正确的头像
                                        video_model_mut.upper_id = submission.upper_id;
                                        video_model_mut.upper_name = submission.upper_name.clone();
                                        video_model_mut.upper_face = matched_staff_face;
                                        collaboration_video_updated = true;
                                        info!(
                                            "合作视频 {} 归类到订阅UP主「{}」(来源：{})",
                                            video_model_mut.bvid,
                                            submission.upper_name,
                                            if video_model.source_submission_id.is_some() {
                                                "投稿订阅"
                                            } else {
                                                "收藏夹"
                                            }
                                        );
                                    } else {
                                        debug!("staff列表中没有找到已订阅的UP主");
                                    }
                                } else {
                                    debug!("staff列表只有{}个成员，不是合作视频", staff_list.len());
                                }
                            } else {
                                debug!("视频 {} 没有staff信息", video_model.bvid);
                            }

                            let txn = connection.begin().await?;
                            // 将分页信息写入数据库
                            create_pages(pages, &video_model_mut, &txn).await?;
                            let mut video_active_model = view_info.into_detail_model(video_model_mut.clone());
                            video_source.set_relation_id(&mut video_active_model);
                            video_active_model.single_page = Set(Some(pages_len == 1));
                            video_active_model.tags = Set(Some(serde_json::to_value(tags)?));

                            // 更新video表的cid字段（从第一个page获取）
                            if let Some(cid) = first_page_cid {
                                video_active_model.cid = Set(Some(cid));
                                debug!("更新视频 {} 的cid: {}", video_model_mut.bvid, cid);
                            }

                            // 只有合作视频更新时才覆盖upper信息，保持其他视频的API更新不被影响
                            if collaboration_video_updated {
                                debug!("合作视频检测到更新，覆盖upper信息到数据库");
                                video_active_model.upper_id = Set(video_model_mut.upper_id);
                                video_active_model.upper_name = Set(video_model_mut.upper_name.clone());
                                video_active_model.upper_face = Set(video_model_mut.upper_face.clone());
                            } else {
                                debug!("非合作视频或未发生更新，保持API返回的upper信息");
                            }

                            video_active_model.save(&txn).await?;
                            txn.commit().await?;
                        }
                    };
                    Ok::<_, anyhow::Error>(())
                }
            })
            .collect::<FuturesUnordered<_>>();

        // 并发执行所有任务
        let mut stream = tasks;
        while let Some(res) = stream.next().await {
            if let Err(e) = res {
                // 使用错误分类器进行统一处理
                #[allow(clippy::needless_borrow)]
                let classified_error = crate::error::ErrorClassifier::classify_error(&e);

                if classified_error.error_type == crate::error::ErrorType::UserCancelled {
                    info!("视频详情获取因用户暂停而终止: {}", classified_error.message);
                    return Err(e); // 直接返回暂停错误，不取消其他任务
                }

                let error_msg = e.to_string();

                if e.downcast_ref::<DownloadAbortError>().is_some() || error_msg.contains("Download cancelled") {
                    token.cancel();
                    // drain the rest of the tasks
                    while stream.next().await.is_some() {}
                    return Err(e);
                }
                // for other errors, just log and continue
                error!("获取视频详情时发生错误: {:#}", e);
            }
        }
        info!("完成普通视频详情处理");
    }
    video_source.log_fetch_video_end();
    Ok(())
}

/// 下载所有未处理成功的视频
pub async fn download_unprocessed_videos(
    bili_client: &BiliClient,
    video_source: &VideoSourceEnum,
    connection: &DatabaseConnection,
    downloader: &UnifiedDownloader,
    token: CancellationToken,
) -> Result<()> {
    // Early exit when paused/cancelled
    if crate::task::TASK_CONTROLLER.is_paused() || token.is_cancelled() {
        info!("任务已暂停/取消，跳过下载阶段");
        return Ok(());
    }
    video_source.log_download_video_start();
    let current_config = crate::config::reload_config();
    let semaphore = Semaphore::new(current_config.concurrent_limit.video);
    let unhandled_videos_pages = filter_unhandled_video_pages(video_source.filter_expr(), connection).await?;

    // 只有当有未处理视频时才显示日志
    if !unhandled_videos_pages.is_empty() {
        info!("找到 {} 个未处理完成的视频", unhandled_videos_pages.len());
    }

    let mut assigned_upper = HashSet::new();
    let mut assigned_bangumi_seasons = HashSet::new();
    let tasks = unhandled_videos_pages
        .into_iter()
        .map(|(video_model, pages_model)| {
            let should_download_upper = if let Some(season_id) = &video_model.season_id {
                // 番剧：基于season_id判断是否需要生成series级别文件
                let season_key = format!("season_{}", season_id);
                let should_download = !assigned_bangumi_seasons.contains(&season_key);
                assigned_bangumi_seasons.insert(season_key);
                debug!("番剧视频「{}」season_id={}, should_download_upper={}",
                       video_model.name, season_id, should_download);
                should_download
            } else {
                // 普通视频：基于upper_id判断
                let should_download = !assigned_upper.contains(&video_model.upper_id);
                assigned_upper.insert(video_model.upper_id);
                should_download
            };
            debug!("下载视频: {}", video_model.name);
            download_video_pages(
                bili_client,
                video_source,
                video_model,
                pages_model,
                connection,
                &semaphore,
                downloader,
                should_download_upper,
                token.clone(),
            )
        })
        .collect::<FuturesUnordered<_>>();
    let mut download_aborted = false;
    let mut stream = tasks;
    // 使用循环和select来处理任务，以便在检测到取消信号时立即停止
    while let Some(res) = stream.next().await {
        match res {
            Ok(model) => {
                if download_aborted {
                    continue;
                }
                // 任务成功完成，更新数据库
                if let Err(db_err) = update_videos_model(vec![model], connection).await {
                    error!("更新数据库失败: {:#}", db_err);
                }
            }
            Err(e) => {
                let error_msg = e.to_string();

                // 调试：输出完整的错误信息
                debug!("检查下载错误消息: '{}'", error_msg);
                debug!("完整错误链: {:#}", e);
                debug!("是否包含'任务已暂停': {}", error_msg.contains("任务已暂停"));
                debug!("是否包含'停止下载': {}", error_msg.contains("停止下载"));
                debug!(
                    "是否包含'Download cancelled': {}",
                    error_msg.contains("Download cancelled")
                );

                // 检查是否是暂停导致的失败，只有在任务暂停时才将 Download cancelled 视为暂停错误
                if error_msg.contains("任务已暂停")
                    || error_msg.contains("停止下载")
                    || error_msg.contains("用户主动暂停任务")
                    || (error_msg.contains("Download cancelled") && crate::task::TASK_CONTROLLER.is_paused())
                {
                    info!("下载任务因用户暂停而终止: {}", error_msg);
                    continue; // 跳过暂停相关的错误，不触发风控
                }

                if e.downcast_ref::<DownloadAbortError>().is_some() || error_msg.contains("Download cancelled") {
                    if !download_aborted {
                        debug!("检测到风控或取消信号，开始中止所有下载任务");
                        token.cancel(); // 立即取消所有其他正在运行的任务
                        download_aborted = true;
                    }
                } else {
                    // 检查是否为暂停相关错误
                    let error_msg = e.to_string();
                    if error_msg.contains("用户主动暂停任务") || error_msg.contains("任务已暂停") {
                        info!("下载任务因用户暂停而终止");
                    } else {
                        // 任务返回了非中止的错误
                        error!("下载任务失败: {:#}", e);
                    }
                }
            }
        }
    }

    if download_aborted {
        error!("下载触发风控，已终止所有任务，停止所有后续扫描");

        // 自动重置风控导致的失败任务
        if let Err(reset_err) = auto_reset_risk_control_failures(connection).await {
            error!("自动重置风控失败任务时出错: {:#}", reset_err);
        }

        video_source.log_download_video_end();
        // 风控时返回错误，中断整个扫描循环
        bail!(DownloadAbortError());
    }
    video_source.log_download_video_end();
    Ok(())
}

/// 对当前循环中失败的视频进行一次重试
pub async fn retry_failed_videos_once(
    bili_client: &BiliClient,
    video_source: &VideoSourceEnum,
    connection: &DatabaseConnection,
    downloader: &UnifiedDownloader,
    token: CancellationToken,
) -> Result<()> {
    // Early exit when paused/cancelled
    if crate::task::TASK_CONTROLLER.is_paused() || token.is_cancelled() {
        info!("任务已暂停/取消，跳过失败视频重试阶段");
        return Ok(());
    }
    let failed_videos_pages = get_failed_videos_in_current_cycle(video_source.filter_expr(), connection).await?;

    if failed_videos_pages.is_empty() {
        debug!("当前循环中没有失败的视频需要重试");
        return Ok(());
    }

    info!("开始重试当前循环中的 {} 个失败视频", failed_videos_pages.len());

    let current_config = crate::config::reload_config();
    let semaphore = Semaphore::new(current_config.concurrent_limit.video);
    let mut assigned_upper = HashSet::new();
    let mut assigned_bangumi_seasons = HashSet::new();

    let tasks = failed_videos_pages
        .into_iter()
        .map(|(video_model, pages_model)| {
            let should_download_upper = if let Some(season_id) = &video_model.season_id {
                // 番剧：基于season_id判断是否需要生成series级别文件
                let season_key = format!("season_{}", season_id);
                let should_download = !assigned_bangumi_seasons.contains(&season_key);
                assigned_bangumi_seasons.insert(season_key);
                debug!("重试番剧视频「{}」season_id={}, should_download_upper={}",
                       video_model.name, season_id, should_download);
                should_download
            } else {
                // 普通视频：基于upper_id判断
                let should_download = !assigned_upper.contains(&video_model.upper_id);
                assigned_upper.insert(video_model.upper_id);
                should_download
            };
            debug!("重试视频: {}", video_model.name);
            download_video_pages(
                bili_client,
                video_source,
                video_model,
                pages_model,
                connection,
                &semaphore,
                downloader,
                should_download_upper,
                token.clone(),
            )
        })
        .collect::<FuturesUnordered<_>>();

    let mut download_aborted = false;
    let mut stream = tasks;
    let mut retry_success_count = 0;

    while let Some(res) = stream.next().await {
        match res {
            Ok(model) => {
                if download_aborted {
                    continue;
                }
                retry_success_count += 1;
                if let Err(db_err) = update_videos_model(vec![model], connection).await {
                    error!("重试后更新数据库失败: {:#}", db_err);
                }
            }
            Err(e) => {
                let error_msg = e.to_string();

                // 检查是否是暂停导致的失败，只有在任务暂停时才将 Download cancelled 视为暂停错误
                if error_msg.contains("任务已暂停")
                    || error_msg.contains("停止下载")
                    || error_msg.contains("用户主动暂停任务")
                    || (error_msg.contains("Download cancelled") && crate::task::TASK_CONTROLLER.is_paused())
                {
                    info!("重试任务因用户暂停而终止: {}", error_msg);
                    continue; // 跳过暂停相关的错误，不触发风控
                }

                if e.downcast_ref::<DownloadAbortError>().is_some() || error_msg.contains("Download cancelled") {
                    if !download_aborted {
                        debug!("重试过程中检测到风控或取消信号，停止重试");
                        token.cancel();
                        download_aborted = true;
                    }
                } else {
                    // 重试失败，但不中断其他重试任务
                    debug!("视频重试失败: {:#}", e);
                }
            }
        }
    }

    if download_aborted {
        warn!("重试过程中触发风控，已停止重试");
        // 不返回错误，避免影响主流程
    } else if retry_success_count > 0 {
        info!("循环内重试完成，成功重试 {} 个视频", retry_success_count);
    } else {
        debug!("循环内重试完成，但没有视频重试成功");
    }

    Ok(())
}

/// 分页下载任务的参数结构体
pub struct DownloadPageArgs<'a> {
    pub should_run: bool,
    pub bili_client: &'a BiliClient,
    pub video_source: &'a VideoSourceEnum,
    pub video_model: &'a video::Model,
    pub pages: Vec<page::Model>,
    pub connection: &'a DatabaseConnection,
    pub downloader: &'a UnifiedDownloader,
    pub base_path: &'a Path,
    #[allow(dead_code)]
    pub token: CancellationToken,
}

#[allow(clippy::too_many_arguments)]
pub async fn download_video_pages(
    bili_client: &BiliClient,
    video_source: &VideoSourceEnum,
    video_model: video::Model,
    pages: Vec<page::Model>,
    connection: &DatabaseConnection,
    semaphore: &Semaphore,
    downloader: &UnifiedDownloader,
    should_download_upper: bool,
    token: CancellationToken,
) -> Result<video::ActiveModel> {
    let _permit = tokio::select! {
        biased;
        _ = token.cancelled() => return Err(anyhow!("Download cancelled")),
        permit = semaphore.acquire() => permit.context("acquire semaphore failed")?,
    };
    let mut status = VideoStatus::from(video_model.download_status);
    let separate_status = status.should_run();

    // 检查是否为合集
    let is_collection = matches!(video_source, VideoSourceEnum::Collection(_));

    // 重新从数据库加载视频信息，以获取可能在fetch_video_details中更新的upper信息
    let final_video_model = if let Ok(Some(updated)) = video::Entity::find_by_id(video_model.id).one(connection).await {
        debug!(
            "重新加载视频信息: upper_name={}, upper_id={}",
            updated.upper_name, updated.upper_id
        );
        updated
    } else {
        debug!("无法重新加载视频信息，使用原始模型");
        video_model.clone()
    };

    // 对于已经获取过详情但可能需要合作视频重新归类的普通视频，进行检测
    let final_video_model = {
        // 检查是否需要进行合作视频检测（只对有staff信息的视频）
        if let Some(staff_info) = &final_video_model.staff_info {
            if let Ok(staff_list) = serde_json::from_value::<Vec<crate::bilibili::StaffInfo>>(staff_info.clone()) {
                debug!(
                    "视频 {} 有staff信息，成员数量: {} (下载阶段检测)",
                    final_video_model.bvid,
                    staff_list.len()
                );

                if staff_list.len() > 1 {
                    // 获取所有启用的订阅
                    let submissions = submission::Entity::find()
                        .filter(submission::Column::Enabled.eq(true))
                        .all(connection)
                        .await
                        .context("get submissions failed")?;

                    let mut matched_submission = None;
                    // 检查staff中是否有已订阅的UP主
                    for submission in &submissions {
                        for staff_member in &staff_list {
                            if staff_member.mid == submission.upper_id {
                                debug!(
                                    "在staff中找到已订阅的UP主：{} ({})",
                                    staff_member.name, staff_member.mid
                                );
                                matched_submission = Some(submission);
                                break;
                            }
                        }
                    }

                    // 如果找到了匹配的订阅UP主，进行归类
                    if let Some(submission) = matched_submission {
                        // 从staff信息中找到匹配UP主的头像
                        let matched_staff_face = staff_list
                            .iter()
                            .find(|staff| staff.mid == submission.upper_id)
                            .map(|staff| staff.face.clone())
                            .unwrap_or_default();

                        debug!(
                            "为合作视频匹配UP主头像 (下载阶段): {} -> {}",
                            submission.upper_name, matched_staff_face
                        );

                        // 创建更新后的视频模型
                        let mut updated_model = final_video_model.clone();
                        updated_model.upper_id = submission.upper_id;
                        updated_model.upper_name = submission.upper_name.clone();
                        updated_model.upper_face = matched_staff_face.clone();

                        // 立即保存到数据库
                        let mut active_model: video::ActiveModel = updated_model.clone().into();
                        active_model.upper_id = Set(submission.upper_id);
                        active_model.upper_name = Set(submission.upper_name.clone());
                        active_model.upper_face = Set(matched_staff_face);

                        if let Err(e) = active_model.update(connection).await {
                            warn!("更新合作视频信息失败: {}", e);
                        } else {
                            // 触发异步同步到内存DB
                            info!(
                                "合作视频 {} 归类到订阅UP主「{}」(下载阶段处理)",
                                updated_model.bvid, submission.upper_name
                            );
                        }

                        updated_model
                    } else {
                        debug!("staff列表中没有找到已订阅的UP主 (下载阶段)");
                        final_video_model
                    }
                } else {
                    debug!("staff列表只有{}个成员，不是合作视频 (下载阶段)", staff_list.len());
                    final_video_model
                }
            } else {
                debug!("解析staff信息失败 (下载阶段)");
                final_video_model
            }
        } else {
            debug!("视频 {} 没有staff信息 (下载阶段)", final_video_model.bvid);
            final_video_model
        }
    };

    // 获取视频源基础路径
    let video_source_base_path = video_source.video_path();

    let path = {
            // 其他类型的视频源使用原来的逻辑
            let base_folder_name = crate::config::with_config(|bundle| {
                bundle.render_video_template(&video_format_args(&final_video_model))
            })
            .map_err(|e| anyhow::anyhow!("模板渲染失败: {}", e))?;

            debug!("普通视频源 - 渲染的文件夹名: '{}'", base_folder_name);
            debug!("普通视频源 - 基础路径: {:?}", video_source_base_path);

            // **智能判断：根据模板内容决定是否需要去重**
            let video_template = crate::config::with_config(|bundle| bundle.config.video_name.as_ref().to_string());
            let needs_deduplication = video_template.contains("title")
                || (video_template.contains("name") && !video_template.contains("upper_name"));

            if needs_deduplication {
                // 智能去重：检查文件夹名是否已存在，如果存在则追加唯一标识符
                let unique_folder_name = generate_unique_folder_name(
                    video_source_base_path,
                    &base_folder_name,
                    &final_video_model,
                    &final_video_model.pubtime.format("%Y-%m-%d").to_string(),
                );
                debug!("使用去重文件夹名: '{}'", unique_folder_name);
                let final_path = video_source_base_path.join(&unique_folder_name);
                debug!("最终计算路径: {:?}", final_path);
                final_path
            } else {
                // 不使用去重，允许多个视频共享同一文件夹
                debug!("不使用去重，直接使用基础文件夹名: '{}'", base_folder_name);
                let final_path = video_source_base_path.join(&base_folder_name);
                debug!("最终计算路径: {:?}", final_path);
                final_path
            }
        };

    // 检查是否为多P视频且启用了Season结构
    let config = crate::config::reload_config();
    let is_single_page = final_video_model.single_page.unwrap_or(true);

    let (base_path, season_folder, _season_root) = if (!is_single_page && config.multi_page_use_season_structure)
        || (is_collection && config.collection_use_season_structure)
    {
        // 为多P视频或合集创建Season文件夹结构
        let season_folder_name = "Season 01".to_string();
        let season_path = path.join(&season_folder_name);
        (season_path, Some(season_folder_name), Some(path))
    } else {
        (path, None, None)
    };

    // 延迟创建季度文件夹，只在实际需要写入文件时创建

    let upper_id = final_video_model.upper_id.to_string();
    let current_config = crate::config::reload_config();
    let base_upper_path = &current_config
        .upper_path
        .join(upper_id.chars().next().context("upper_id is empty")?.to_string())
        .join(upper_id);
    let is_single_page = final_video_model.single_page.context("single_page is null")?;

    // 为多P视频生成基于视频名称的文件名
    let video_base_name = if !is_single_page {
        // 多P视频启用Season结构时，使用视频根目录的文件夹名作为系列级封面的文件名
        let config = crate::config::reload_config();
        if config.multi_page_use_season_structure {
            // 从base_path获取视频根目录的文件夹名称
            if let Some(parent) = base_path.parent() {
                if let Some(folder_name) = parent.file_name() {
                    folder_name.to_string_lossy().to_string()
                } else {
                    final_video_model.name.clone() // 回退到视频标题
                }
            } else {
                final_video_model.name.clone() // 回退到视频标题
            }
        } else {
            // 不使用Season结构时，使用模板渲染
            crate::config::with_config(|bundle| bundle.render_video_template(&video_format_args(&final_video_model)))
                .map_err(|e| anyhow::anyhow!("模板渲染失败: {}", e))?
        }
    } else if is_collection {
        // 合集中的单页视频：检查是否启用Season结构
        let config = crate::config::reload_config();
        if config.collection_use_season_structure {
            // 合集启用Season结构时，使用合集名称作为文件名前缀
            if let VideoSourceEnum::Collection(collection_source) = video_source {
                // 对合集名称进行安全化处理，避免poster/fanart文件名包含斜杠导致创建子文件夹
                let safe_collection_name = crate::utils::filenamify::filenamify(&collection_source.name);
                debug!(
                    "合集poster/fanart文件名安全化 - 原名称: '{}', 安全化后: '{}'",
                    collection_source.name, safe_collection_name
                );
                safe_collection_name
            } else {
                String::new()
            }
        } else {
            String::new() // 合集不使用Season结构时不需要
        }
    } else {
        String::new() // 单P视频不需要这些文件
    };

    // 对于单页视频，page 的下载已经足够
    // 对于多页视频，page 下载仅包含了分集内容，需要额外补上视频的 poster 的 tvshow.nfo
    // 使用 tokio::join! 替代装箱的 Future，零分配并行执行

    // 首先检查是否取消
    if token.is_cancelled() {
        return Err(anyhow!("Download cancelled"));
    }

    // 为启用Season结构的合集视频判断是否下载封面（依赖数据库状态，不检查文件存在性）
    let should_download_season_poster = {
        let config = crate::config::reload_config();
        let uses_season_structure = (is_collection && config.collection_use_season_structure)
            || (!is_single_page && config.multi_page_use_season_structure);

        if uses_season_structure && season_folder.is_some() {
            // 对于合集，只有第一个视频才下载合集封面
            if is_collection {
                if let VideoSourceEnum::Collection(collection_source) = video_source {
                    match get_collection_video_episode_number(connection, collection_source.id, &video_model.bvid).await
                    {
                        Ok(episode_number) => {
                            let is_first_episode = episode_number == 1;
                            info!(
                                "合集「{}」视频「{}」集数检查: episode={}, is_first={}",
                                collection_source.name, video_model.name, episode_number, is_first_episode
                            );
                            is_first_episode
                        }
                        Err(e) => {
                            warn!("获取合集视频集数失败: {}, 跳过合集封面下载", e);
                            false
                        }
                    }
                } else {
                    false
                }
            } else {
                true // 非合集的多P视频，依赖should_run参数控制
            }
        } else {
            true // 未启用Season结构时不进行检查
        }
    };

    // 先处理NFO生成（独立执行，避免tokio::join!类型问题）
    let nfo_result = {
        // 普通视频：使用原有逻辑
        // 对于合集，只在第一个视频时生成tvshow.nfo，避免重复生成
        let should_generate_nfo = if is_collection {
            // 合集：只有第一个视频时生成tvshow.nfo
            let config = crate::config::reload_config();
            if separate_status[2] && config.collection_use_season_structure {
                // 检查是否为第一个视频
                if let VideoSourceEnum::Collection(collection_source) = video_source {
                    match get_collection_video_episode_number(connection, collection_source.id, &video_model.bvid).await
                    {
                        Ok(episode_number) => episode_number == 1,
                        Err(_) => false,
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            // 普通视频：为多P视频生成nfo
            separate_status[2] && !is_single_page
        };

        if should_generate_nfo && is_collection {
            // 合集：使用带合集信息的NFO生成（第一个视频时）
            if let VideoSourceEnum::Collection(collection_source) = video_source {
                // 先获取合集封面信息
                let collection_cover = match collection::Entity::find_by_id(collection_source.id)
                    .one(connection)
                    .await
                {
                    Ok(Some(fresh_collection)) => fresh_collection.cover.clone(),
                    _ => None,
                };

                generate_collection_video_nfo(
                    true,
                    &video_model,
                    Some(&collection_source.name),
                    collection_cover.as_deref(),
                    // 多P视频或合集使用Season结构时，tvshow.nfo放在视频根目录
                    {
                        let config = crate::config::reload_config();
                        if ((!is_single_page && config.multi_page_use_season_structure)
                            || (is_collection && config.collection_use_season_structure))
                            && season_folder.is_some()
                        {
                            // 需要从base_path（Season文件夹）回到父目录（视频根目录）
                            base_path
                                .parent()
                                .map(|parent| parent.join("tvshow.nfo"))
                                .unwrap_or_else(|| base_path.join("tvshow.nfo"))
                        } else {
                            // 普通视频nfo放在视频文件夹
                            base_path.join(format!("{}.nfo", video_base_name))
                        }
                    },
                )
                .await
            } else {
                // 不应该到这里
                Ok(ExecutionStatus::Skipped)
            }
        } else {
            // 普通视频：使用原有逻辑
            generate_video_nfo(
                should_generate_nfo,
                &video_model,
                // 多P视频或合集使用Season结构时，tvshow.nfo放在视频根目录
                {
                    let config = crate::config::reload_config();
                    if ((!is_single_page && config.multi_page_use_season_structure)
                        || (is_collection && config.collection_use_season_structure))
                        && season_folder.is_some()
                    {
                        // 需要从base_path（Season文件夹）回到父目录（视频根目录）
                        base_path
                            .parent()
                            .map(|parent| parent.join("tvshow.nfo"))
                            .unwrap_or_else(|| base_path.join("tvshow.nfo"))
                    } else {
                        // 普通视频nfo放在视频文件夹
                        base_path.join(format!("{}.nfo", video_base_name))
                    }
                },
            )
            .await
        }
    };

    // 预先获取合集封面URL（如果需要）
    let collection_cover_url = if is_collection && should_download_season_poster {
        if let VideoSourceEnum::Collection(collection_source) = video_source {
            match get_collection_video_episode_number(connection, collection_source.id, &video_model.bvid).await {
                Ok(1) => {
                    // 第一个视频时从数据库重新获取最新的合集信息
                    match collection::Entity::find_by_id(collection_source.id)
                        .one(connection)
                        .await
                    {
                        Ok(Some(fresh_collection)) => match fresh_collection.cover.as_ref() {
                            Some(cover_url) if !cover_url.is_empty() => {
                                info!("合集「{}」使用数据库保存的封面: {}", fresh_collection.name, cover_url);
                                Some(cover_url.clone())
                            }
                            _ => {
                                info!("合集「{}」数据库中无封面URL，使用视频封面", fresh_collection.name);
                                None
                            }
                        },
                        Ok(None) => {
                            warn!("合集ID {} 在数据库中不存在", collection_source.id);
                            None
                        }
                        Err(e) => {
                            warn!("查询合集信息失败: {}", e);
                            None
                        }
                    }
                }
                _ => {
                    // 非第一个视频，不需要重复获取
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // 番剧相关功能已移除
    let season_nfo_result = Ok(ExecutionStatus::Skipped);

    // 番剧相关功能已移除
    let season_images_result = Some(ExecutionStatus::Skipped);

    let (res_1, res_2, res_3, res_4, res_5) = tokio::join!(
        // 下载视频封面（普通视频）
        fetch_video_poster(
            {
                // 普通视频：为多P视频或启用Season结构的合集生成封面，并检查文件是否已存在
                let config = crate::config::reload_config();
                separate_status[0]
                    && (!is_single_page || (is_collection && config.collection_use_season_structure))
                    && should_download_season_poster
            },
            &video_model,
            downloader,
            {
                // 多P视频或合集使用Season结构时，封面放在视频根目录
                let config = crate::config::reload_config();
                if (!is_single_page && config.multi_page_use_season_structure && season_folder.is_some())
                    || (is_collection && config.collection_use_season_structure && season_folder.is_some())
                {
                    // 需要从base_path（Season文件夹）回到父目录（视频根目录）
                    base_path
                        .parent()
                        .map(|parent| parent.join(format!("{}-thumb.jpg", video_base_name)))
                        .unwrap_or_else(|| base_path.join(format!("{}-thumb.jpg", video_base_name)))
                } else {
                    // 普通视频封面放在视频文件夹
                    base_path.join(format!("{}-thumb.jpg", video_base_name))
                }
            },
            {
                // 多P视频或合集使用Season结构时，fanart放在视频根目录
                let config = crate::config::reload_config();
                if (!is_single_page && config.multi_page_use_season_structure && season_folder.is_some())
                    || (is_collection && config.collection_use_season_structure && season_folder.is_some())
                {
                    // 需要从base_path（Season文件夹）回到父目录（视频根目录）
                    base_path
                        .parent()
                        .map(|parent| parent.join(format!("{}-fanart.jpg", video_base_name)))
                        .unwrap_or_else(|| base_path.join(format!("{}-fanart.jpg", video_base_name)))
                } else {
                    // 普通视频fanart放在视频文件夹
                    base_path.join(format!("{}-fanart.jpg", video_base_name))
                }
            },
            token.clone(),
            // thumb URL选择逻辑：合集和普通视频使用默认封面
            if let Some(ref cover_url) = collection_cover_url {
                Some(cover_url.as_str())
            } else {
                None
            },
            // fanart URL选择逻辑：普通视频复用poster
            None,
        ),
        // 番剧主封面功能已移除（始终跳过）
        fetch_bangumi_poster(
            false,
            &video_model,
            downloader,
            std::path::PathBuf::from("/dev/null"),
            token.clone(),
            None,
        ),
        // 下载 Up 主头像
        fetch_upper_face(
            separate_status[2] && should_download_upper,
            &final_video_model,
            downloader,
            base_upper_path.join("folder.jpg"),
            token.clone(),
        ),
        // 生成 Up 主信息的 nfo
        generate_upper_nfo(
            separate_status[3] && should_download_upper,
            &final_video_model,
            base_upper_path.join("person.nfo"),
        ),
        // 分发并执行分 P 下载的任务
        dispatch_download_page(
            DownloadPageArgs {
                should_run: separate_status[4],
                bili_client,
                video_source,
                video_model: &final_video_model,
                pages,
                connection,
                downloader,
                base_path: &base_path,
                token: token.clone(),
            },
            token.clone()
        )
    );

    // 主要的5个任务结果，保持与VideoStatus<5>兼容
    let main_results = [res_1, nfo_result, res_3, res_4, res_5]
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
    status.update_status(&main_results);

    // 额外的结果单独处理（番剧相关功能已移除，仅保留兼容性）
    let extra_results = [
        Ok(ExecutionStatus::Skipped), // 原season_nfo_result
        Ok(ExecutionStatus::Skipped), // 原season_images_result
        res_2, // 原番剧主封面 poster.jpg 的结果（已禁用）
    ]
    .into_iter()
    .map(Into::into)
    .collect::<Vec<_>>();

    // 合并所有结果用于日志处理
    let mut all_results = main_results;
    all_results.extend(extra_results);

    // 充电视频在获取详情时已经被upower字段检测并处理，无需后期检测

    all_results
        .iter()
        .take(7)
        .zip([
            "封面",
            "详情",
            "作者头像",
            "作者详情",
            "分页下载",
            "季度NFO",
            "季度图片",
        ])
        .for_each(|(res, task_name)| match res {
            ExecutionStatus::Skipped => debug!("处理视频「{}」{}已成功过，跳过", &video_model.name, task_name),
            ExecutionStatus::Succeeded => debug!("处理视频「{}」{}成功", &video_model.name, task_name),
            ExecutionStatus::Ignored(e) => {
                let error_msg = e.to_string();
                if !error_msg.contains("status code: 87007") {
                    info!(
                        "处理视频「{}」{}出现常见错误，已忽略: {:#}",
                        &video_model.name, task_name, e
                    );
                }
            }
            ExecutionStatus::ClassifiedFailed(classified_error) => {
                // 根据错误分类进行不同级别的日志记录
                match classified_error.error_type {
                    crate::error::ErrorType::NotFound => {
                        debug!(
                            "处理视频「{}」{}失败({}): {}",
                            &video_model.name, task_name, classified_error.error_type, classified_error.message
                        );
                    }
                    crate::error::ErrorType::Permission => {
                        // 权限错误（充电专享视频现在在获取详情时处理）
                        info!(
                            "跳过视频「{}」{}: {}",
                            &video_model.name, task_name, classified_error.message
                        );
                    }
                    crate::error::ErrorType::Network
                    | crate::error::ErrorType::Timeout
                    | crate::error::ErrorType::RateLimit => {
                        warn!(
                            "处理视频「{}」{}失败({}): {}{}",
                            &video_model.name,
                            task_name,
                            classified_error.error_type,
                            classified_error.message,
                            if classified_error.should_retry {
                                " (可重试)"
                            } else {
                                ""
                            }
                        );
                    }
                    crate::error::ErrorType::RiskControl => {
                        error!(
                            "处理视频「{}」{}触发风控: {}",
                            &video_model.name, task_name, classified_error.message
                        );
                    }
                    crate::error::ErrorType::UserCancelled => {
                        info!("处理视频「{}」{}因用户暂停而终止", &video_model.name, task_name);
                    }
                    _ => {
                        error!(
                            "处理视频「{}」{}失败({}): {}",
                            &video_model.name, task_name, classified_error.error_type, classified_error.message
                        );
                    }
                }
            }
            ExecutionStatus::Failed(e) | ExecutionStatus::FixedFailed(_, e) => {
                // 使用错误分类器进行统一处理
                #[allow(clippy::needless_borrow)]
                let classified_error = crate::error::ErrorClassifier::classify_error(&e);
                match classified_error.error_type {
                    crate::error::ErrorType::NotFound => {
                        debug!("处理视频「{}」{}失败(404): {:#}", &video_model.name, task_name, e);
                    }
                    crate::error::ErrorType::UserCancelled => {
                        info!("处理视频「{}」{}因用户暂停而终止", &video_model.name, task_name);
                    }
                    _ => {
                        // 对于分页下载任务，错误日志已经在内部处理了，这里只记录DEBUG级别
                        if task_name == "分页下载" {
                            debug!("处理视频「{}」{}失败: {:#}", &video_model.name, task_name, e);
                        } else {
                            error!("处理视频「{}」{}失败: {:#}", &video_model.name, task_name, e);
                        }
                    }
                }
            }
        });
    if let ExecutionStatus::Failed(e) = all_results
        .into_iter()
        .nth(4)
        .context("page download result not found")?
    {
        if e.downcast_ref::<DownloadAbortError>().is_some() {
            return Err(e);
        }
    }
    let mut video_active_model: video::ActiveModel = final_video_model.into();
    video_active_model.download_status = Set(status.into());

    // 对于多P视频使用Season结构时，保存根文件夹路径而不是Season文件夹路径
    let path_to_save = {
        // 检查是否为多P视频或合集且启用了Season结构
        let config = crate::config::reload_config();
        if (!is_single_page && config.multi_page_use_season_structure && season_folder.is_some())
            || (is_collection && config.collection_use_season_structure && season_folder.is_some())
        {
            // 对于多P视频或合集使用Season结构时，保存根目录路径而不是Season子文件夹路径
            base_path
                .parent()
                .map(|parent| parent.to_string_lossy().to_string())
                .unwrap_or_else(|| base_path.to_string_lossy().to_string())
        } else {
            base_path.to_string_lossy().to_string()
        }
    };

    debug!("=== 路径保存 ===");
    debug!("最终保存到数据库的路径: {:?}", path_to_save);
    debug!("原始基础路径: {:?}", base_path);
    if let Some(ref season_folder) = season_folder {
        debug!("季度文件夹名: {}", season_folder);
    }
    debug!("=== 路径计算结束 ===");

    video_active_model.path = Set(path_to_save);
    Ok(video_active_model)
}

/// 分发并执行分页下载任务，当且仅当所有分页成功下载或达到最大重试次数时返回 Ok，否则根据失败原因返回对应的错误
pub async fn dispatch_download_page(args: DownloadPageArgs<'_>, token: CancellationToken) -> Result<ExecutionStatus> {
    if !args.should_run {
        return Ok(ExecutionStatus::Skipped);
    }

    let current_config = crate::config::reload_config();
    let child_semaphore = Arc::new(Semaphore::new(current_config.concurrent_limit.page));
    let tasks = args
        .pages
        .into_iter()
        .map(|page_model| {
            let page_pid = page_model.pid; // 保存分页ID
            let page_name = page_model.name.clone(); // 保存分页名称
            let semaphore_clone = child_semaphore.clone();
            let token_clone = token.clone();
            let bili_client = args.bili_client;
            let video_source = args.video_source;
            let video_model = args.video_model;
            let connection = args.connection;
            let downloader = args.downloader;
            let base_path = args.base_path;
            async move {
                let result = download_page(
                    bili_client,
                    video_source,
                    video_model,
                    page_model,
                    connection,
                    semaphore_clone.as_ref(),
                    downloader,
                    base_path,
                    token_clone,
                )
                .await;
                // 返回结果和分页信息
                (result, page_pid, page_name)
            }
        })
        .collect::<FuturesUnordered<_>>();
    let (mut download_aborted, mut target_status) = (false, STATUS_OK);
    let mut failed_pages: Vec<String> = Vec::new(); // 收集失败的分页信息
    let mut stream = tasks;
    while let Some((res, page_pid, page_name)) = stream.next().await {
        match res {
            Ok(model) => {
                if download_aborted {
                    continue;
                }
                // 该视频的所有分页的下载状态都会在此返回，需要根据这些状态确认视频层"分 P 下载"子任务的状态
                // 在过去的实现中，此处仅仅根据 page_download_status 的最高标志位来判断，如果最高标志位是 true 则认为完成
                // 这样会导致即使分页中有失败到 MAX_RETRY 的情况，视频层的分 P 下载状态也会被认为是 Succeeded，不够准确
                // 新版本实现会将此处取值为所有子任务状态的最小值，这样只有所有分页的子任务全部成功时才会认为视频层的分 P 下载状态是 Succeeded
                let page_download_status = model.download_status.try_as_ref().expect("download_status must be set");
                let separate_status: [u32; 5] = PageStatus::from(*page_download_status).into();
                for status in separate_status {
                    target_status = target_status.min(status);
                }
                update_pages_model(vec![model], args.connection).await?;
            }
            Err(e) => {
                let error_msg = e.to_string();
                debug!("分页下载错误原始信息 - 第{}页 {}: {}", page_pid, page_name, error_msg);

                // 1. 首先检查是否是用户暂停导致的错误
                if error_msg.contains("任务已暂停")
                    || error_msg.contains("停止下载")
                    || error_msg.contains("用户主动暂停任务")
                    || (error_msg.contains("Download cancelled") && crate::task::TASK_CONTROLLER.is_paused())
                {
                    info!(
                        "分页下载任务因用户暂停而终止 - 第{}页 {}: {}",
                        page_pid, page_name, error_msg
                    );
                    continue; // 跳过暂停相关的错误，不触发风控或其他处理
                }

                // 2. 检查是否是真正的风控错误（DownloadAbortError）
                if e.downcast_ref::<DownloadAbortError>().is_some() {
                    warn!(
                        "检测到真正的风控错误，中止所有下载任务 - 第{}页 {}",
                        page_pid, page_name
                    );
                    if !download_aborted {
                        token.cancel();
                        download_aborted = true;
                    }
                    continue;
                }

                // 充电视频在获取详情时已经被upower字段检测并处理，这里不应该再出现充电视频错误

                // 4. 处理其他类型的错误（包括普通的Download cancelled）
                // 记录更详细的错误信息，包括错误链
                error!("下载分页子任务失败 - 第{}页 {}: {:#}", page_pid, page_name, e);

                // 输出错误链中的所有错误信息
                let mut error_chain = String::new();
                let mut current_error: &dyn std::error::Error = &*e;
                error_chain.push_str(&format!("错误: {}", current_error));

                while let Some(source) = current_error.source() {
                    error_chain.push_str(&format!("\n  原因: {}", source));
                    current_error = source;
                }

                error!("完整错误链: {}", error_chain);

                // 收集失败信息，包含分页标识
                failed_pages.push(format!("第{}页 {}: {}", page_pid, page_name, e));

                // 如果失败的任务没有达到 STATUS_OK，记录当前状态
                if target_status != STATUS_OK {
                    error!("当前分页下载状态: {}, 视频: {}", target_status, &args.video_model.name);
                }
            }
        }
    }

    if download_aborted {
        error!(
            "下载视频「{}」的分页时触发风控，将异常向上传递..",
            &args.video_model.name
        );
        bail!(DownloadAbortError());
    }
    if target_status != STATUS_OK {
        // 充电视频在获取详情时已经被upower字段检测并处理，这里不需要特殊的充电视频逻辑

        // 提供更详细的错误信息，保留原始错误上下文
        error!(
            "视频「{}」分页下载失败，状态码: {}",
            &args.video_model.name, target_status
        );

        // 构建详细的错误信息
        let details = if !failed_pages.is_empty() {
            format!("失败的分页: {}", failed_pages.join("; "))
        } else {
            "请检查网络连接、文件系统权限或重试下载。".to_string()
        };

        // 返回ProcessPageError，携带详细信息
        let process_error = ProcessPageError::new(args.video_model.name.clone(), target_status).with_details(details);
        return Err(process_error.into());
    }
    Ok(ExecutionStatus::Succeeded)
}

/// 下载某个分页，未发生风控且正常运行时返回 Ok(Page::ActiveModel)，其中 status 字段存储了新的下载状态，发生风控时返回 DownloadAbortError
#[allow(clippy::too_many_arguments)]
pub async fn download_page(
    bili_client: &BiliClient,
    video_source: &VideoSourceEnum,
    video_model: &video::Model,
    page_model: page::Model,
    connection: &DatabaseConnection,
    semaphore: &Semaphore,
    downloader: &UnifiedDownloader,
    base_path: &Path,
    token: CancellationToken,
) -> Result<page::ActiveModel> {
    let _permit = tokio::select! {
        biased;
        _ = token.cancelled() => return Err(anyhow!("Download cancelled")),
        permit = semaphore.acquire() => permit.context("acquire semaphore failed")?,
    };
    let mut status = PageStatus::from(page_model.download_status);
    let separate_status = status.should_run();
    let is_single_page = video_model.single_page.context("single_page is null")?;


    // 根据视频源类型选择不同的模板渲染方式
    let base_name = if let VideoSourceEnum::Collection(collection_source) = video_source {
        // 合集视频的特殊处理
        let config = crate::config::reload_config();
        if config.collection_folder_mode.as_ref() == "unified" {
            // 统一模式：使用S01E01格式命名
            match get_collection_video_episode_number(connection, collection_source.id, &video_model.bvid).await {
                Ok(episode_number) => {
                    let clean_name = crate::utils::filenamify::filenamify(&video_model.name);
                    // 检查是否为多P视频
                    let is_single_page = video_model.single_page.unwrap_or(true);
                    if !is_single_page {
                        // 多P视频：在集数后添加分P标识
                        format!("S01E{:02}P{:02} - {}", episode_number, page_model.pid, clean_name)
                    } else {
                        // 单P视频：保持原有格式
                        format!("S01E{:02} - {}", episode_number, clean_name)
                    }
                }
                Err(_) => {
                    // 如果获取序号失败，使用默认命名
                    crate::config::with_config(|bundle| {
                        bundle.render_page_template(&page_format_args(video_model, &page_model))
                    })
                    .map_err(|e| anyhow::anyhow!("模板渲染失败: {}", e))?
                }
            }
        } else {
            // 分离模式：检查是否为多P视频
            let is_single_page = video_model.single_page.unwrap_or(true);
            if !is_single_page {
                // 多P视频：使用multi_page_name模板
                let page_args = page_format_args(video_model, &page_model);
                match crate::config::with_config(|bundle| bundle.render_multi_page_template(&page_args)) {
                    Ok(rendered) => rendered,
                    Err(_) => {
                        // 如果渲染失败，使用默认格式
                        let season_number = 1;
                        let episode_number = page_model.pid;
                        format!("S{:02}E{:02}-{:02}", season_number, episode_number, episode_number)
                    }
                }
            } else {
                // 单P视频：使用page_name模板
                crate::config::with_config(|bundle| {
                    bundle.render_page_template(&page_format_args(video_model, &page_model))
                })
                .map_err(|e| anyhow::anyhow!("模板渲染失败: {}", e))?
            }
        }
    } else if !is_single_page {
        // 对于多P视频（非番剧），使用最新配置中的multi_page_name模板
        let page_args = page_format_args(video_model, &page_model);
        match crate::config::with_config(|bundle| bundle.render_multi_page_template(&page_args)) {
            Ok(rendered) => rendered,
            Err(_) => {
                // 如果渲染失败，使用默认格式
                let season_number = 1;
                let episode_number = page_model.pid;
                format!("S{:02}E{:02}-{:02}", season_number, episode_number, episode_number)
            }
        }
    } else {
        // 单P视频使用最新配置的page_name模板
        crate::config::with_config(|bundle| bundle.render_page_template(&page_format_args(video_model, &page_model)))
            .map_err(|e| anyhow::anyhow!("模板渲染失败: {}", e))?
    };

    let (poster_path, video_path, nfo_path, danmaku_path, fanart_path, subtitle_path) = if is_single_page {
        (
            base_path.join(format!("{}-thumb.jpg", &base_name)),
            base_path.join(format!("{}.mp4", &base_name)),
            base_path.join(format!("{}.nfo", &base_name)),
            base_path.join(format!("{}.zh-CN.default.ass", &base_name)),
            Some(base_path.join(format!("{}-fanart.jpg", &base_name))),
            base_path.join(format!("{}.srt", &base_name)),
        )
    } else {
        // 多P视频直接使用基础路径，不创建子文件夹
        (
            base_path.join(format!("{}-thumb.jpg", &base_name)),
            base_path.join(format!("{}.mp4", &base_name)),
            base_path.join(format!("{}.nfo", &base_name)),
            base_path.join(format!("{}.zh-CN.default.ass", &base_name)),
            // 多P视频的每个分页都应该有自己的fanart
            Some(base_path.join(format!("{}-fanart.jpg", &base_name))),
            base_path.join(format!("{}.srt", &base_name)),
        )
    };
    let dimension = match (page_model.width, page_model.height) {
        (Some(width), Some(height)) => Some(Dimension {
            width,
            height,
            rotate: 0,
        }),
        _ => None,
    };
    let page_info = PageInfo {
        cid: page_model.cid,
        duration: page_model.duration,
        dimension,
        ..Default::default()
    };
    // 使用 tokio::join! 替代装箱的 Future，零分配并行执行
    let (res_1, res_2, res_3, res_4, res_5) = tokio::join!(
        fetch_page_poster(
            separate_status[0],
            video_model,
            &page_model,
            downloader,
            poster_path,
            fanart_path,
            token.clone(),
        ),
        fetch_page_video(
            separate_status[1],
            bili_client,
            video_model,
            downloader,
            &page_info,
            &video_path,
            token.clone(),
        ),
        generate_page_nfo(separate_status[2], video_model, &page_model, nfo_path, connection),
        fetch_page_danmaku(
            video_source.download_danmaku(),
            separate_status[3],
            bili_client,
            video_model,
            &page_info,
            danmaku_path,
            token.clone(),
        ),
        fetch_page_subtitle(
            separate_status[4],
            bili_client,
            video_model,
            &page_info,
            &subtitle_path,
            token.clone(),
        )
    );

    let results = [res_1, res_2, res_3, res_4, res_5]
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
    status.update_status(&results);

    // 充电视频在获取详情时已经被upower字段检测并处理，无需分页级别的后期检测

    results
        .iter()
        .zip(["封面", "视频", "详情", "弹幕", "字幕"])
        .for_each(|(res, task_name)| match res {
            ExecutionStatus::Skipped => debug!(
                "处理视频「{}」第 {} 页{}已成功过，跳过",
                &video_model.name, page_model.pid, task_name
            ),
            ExecutionStatus::Succeeded => debug!(
                "处理视频「{}」第 {} 页{}成功",
                &video_model.name, page_model.pid, task_name
            ),
            ExecutionStatus::Ignored(e) => {
                let error_msg = e.to_string();
                if !error_msg.contains("status code: 87007") {
                    info!(
                        "处理视频「{}」第 {} 页{}出现常见错误，已忽略: {:#}",
                        &video_model.name, page_model.pid, task_name, e
                    );
                }
            }
            ExecutionStatus::ClassifiedFailed(classified_error) => {
                // 根据错误分类进行不同级别的日志记录
                match classified_error.error_type {
                    crate::error::ErrorType::NotFound => {
                        debug!(
                            "处理视频「{}」第 {} 页{}失败({}): {}",
                            &video_model.name,
                            page_model.pid,
                            task_name,
                            classified_error.error_type,
                            classified_error.message
                        );
                    }
                    crate::error::ErrorType::Permission => {
                        // 权限错误（充电专享视频现在在获取详情时处理）
                        info!(
                            "跳过视频「{}」第 {} 页{}: {}",
                            &video_model.name, page_model.pid, task_name, classified_error.message
                        );
                    }
                    crate::error::ErrorType::Network
                    | crate::error::ErrorType::Timeout
                    | crate::error::ErrorType::RateLimit => {
                        warn!(
                            "处理视频「{}」第 {} 页{}失败({}): {}{}",
                            &video_model.name,
                            page_model.pid,
                            task_name,
                            classified_error.error_type,
                            classified_error.message,
                            if classified_error.should_retry {
                                " (可重试)"
                            } else {
                                ""
                            }
                        );
                    }
                    crate::error::ErrorType::RiskControl => {
                        error!(
                            "处理视频「{}」第 {} 页{}触发风控: {}",
                            &video_model.name, page_model.pid, task_name, classified_error.message
                        );
                    }
                    crate::error::ErrorType::UserCancelled => {
                        info!(
                            "处理视频「{}」第 {} 页{}因用户暂停而终止",
                            &video_model.name, page_model.pid, task_name
                        );
                    }
                    _ => {
                        error!(
                            "处理视频「{}」第 {} 页{}失败({}): {}",
                            &video_model.name,
                            page_model.pid,
                            task_name,
                            classified_error.error_type,
                            classified_error.message
                        );
                    }
                }
            }
            ExecutionStatus::Failed(e) | ExecutionStatus::FixedFailed(_, e) => {
                // 使用错误分类器进行统一处理
                #[allow(clippy::needless_borrow)]
                let classified_error = crate::error::ErrorClassifier::classify_error(&e);
                match classified_error.error_type {
                    crate::error::ErrorType::NotFound => {
                        debug!(
                            "处理视频「{}」第 {} 页{}失败(404): {:#}",
                            &video_model.name, page_model.pid, task_name, e
                        );
                    }
                    crate::error::ErrorType::UserCancelled => {
                        info!(
                            "处理视频「{}」第 {} 页{}因用户暂停而终止",
                            &video_model.name, page_model.pid, task_name
                        );
                    }
                    _ => {
                        error!(
                            "处理视频「{}」第 {} 页{}失败: {:#}",
                            &video_model.name, page_model.pid, task_name, e
                        );
                    }
                }
            }
        });
    // 检查下载视频时是否触发风控
    match results.into_iter().nth(1).context("video download result not found")? {
        ExecutionStatus::Failed(e) => {
            if let Ok(BiliError::RiskControlOccurred) = e.downcast::<BiliError>() {
                bail!(DownloadAbortError());
            }
        }
        ExecutionStatus::ClassifiedFailed(ref classified_error) => {
            if classified_error.error_type == crate::error::ErrorType::RiskControl {
                bail!(DownloadAbortError());
            }
        }
        _ => {}
    }
    let mut page_active_model: page::ActiveModel = page_model.into();
    page_active_model.download_status = Set(status.into());
    page_active_model.path = Set(Some(video_path.to_string_lossy().to_string()));
    Ok(page_active_model)
}

pub async fn fetch_page_poster(
    should_run: bool,
    video_model: &video::Model,
    page_model: &page::Model,
    downloader: &UnifiedDownloader,
    poster_path: PathBuf,
    fanart_path: Option<PathBuf>,
    token: CancellationToken,
) -> Result<ExecutionStatus> {
    if !should_run {
        return Ok(ExecutionStatus::Skipped);
    }
    let single_page = video_model.single_page.context("single_page is null")?;
    let url = if single_page {
        // 单页视频直接用视频的封面
        video_model.cover.as_str()
    } else {
        // 多页视频，如果单页没有封面，就使用视频的封面
        match &page_model.image {
            Some(url) => url.as_str(),
            None => video_model.cover.as_str(),
        }
    };
    let urls = vec![url];
    tokio::select! {
        biased;
        _ = token.cancelled() => return Ok(ExecutionStatus::Skipped),
        res = downloader.fetch_with_fallback(&urls, &poster_path) => res,
    }?;
    if let Some(fanart_path) = fanart_path {
        ensure_parent_dir_for_file(&fanart_path).await?;
        fs::copy(&poster_path, &fanart_path).await?;
    }
    Ok(ExecutionStatus::Succeeded)
}

/// 下载单个流文件并返回文件大小（使用UnifiedDownloader智能选择下载方式）
async fn download_stream(downloader: &UnifiedDownloader, urls: &[&str], path: &Path) -> Result<u64> {
    // 直接使用UnifiedDownloader，它会智能选择aria2或原生下载器
    // aria2本身就支持多线程，原生下载器作为备选方案使用单线程
    let download_result = downloader.fetch_with_fallback(urls, path).await;

    match download_result {
        Ok(_) => {
            // 获取文件大小
            Ok(tokio::fs::metadata(path)
                .await
                .map(|metadata| metadata.len())
                .unwrap_or(0))
        }
        Err(e) => {
            let error_msg = e.to_string();
            // 检查是否为暂停相关错误
            if error_msg.contains("用户主动暂停任务") || error_msg.contains("任务已暂停") {
                info!("下载因用户暂停而终止");
            } else if let Some(BiliError::RequestFailed(-404, _)) = e.downcast_ref::<BiliError>() {
                // 对于404错误，降级为debug日志
                debug!("下载失败(404): {:#}", e);
            } else {
                // 使用错误分类器进行统一处理
                #[allow(clippy::needless_borrow)]
                let classified_error = crate::error::ErrorClassifier::classify_error(&e);
                match classified_error.error_type {
                    crate::error::ErrorType::UserCancelled => {
                        info!("下载因用户暂停而终止: {:#}", e);
                    }
                    _ => {
                        error!("下载失败: {:#}", e);
                    }
                }
            }
            Err(e)
        }
    }
}

pub async fn fetch_page_video(
    should_run: bool,
    bili_client: &BiliClient,
    video_model: &video::Model,
    downloader: &UnifiedDownloader,
    page_info: &PageInfo,
    page_path: &Path,
    token: CancellationToken,
) -> Result<ExecutionStatus> {
    if !should_run {
        return Ok(ExecutionStatus::Skipped);
    }

    let bili_video = Video::new(bili_client, video_model.bvid.clone());

    // 获取视频流信息 - 使用带API降级机制的调用
    let mut streams = tokio::select! {
        biased;
        _ = token.cancelled() => return Err(anyhow!("Download cancelled")),
        res = async {
            // 检查是否为番剧视频
            if video_model.source_type == Some(1) && video_model.ep_id.is_some() {
                // 番剧视频使用番剧专用API的回退机制
                let ep_id = video_model.ep_id.as_ref().unwrap();
                debug!("使用带质量回退的番剧API获取播放地址: ep_id={}", ep_id);
                bili_video.get_bangumi_page_analyzer_with_fallback(page_info, ep_id).await
            } else {
                // 普通视频使用API降级机制（普通视频API -> 番剧API）
                debug!("使用API降级机制获取播放地址（普通视频API -> 番剧API）");
                // 传递ep_id以便在需要时降级到番剧API，如果没有ep_id则会自动从视频详情API获取
                let ep_id = video_model.ep_id.as_deref();
                if ep_id.is_some() {
                    debug!("视频已有ep_id: {:?}，可直接用于API降级", ep_id);
                } else {
                    debug!("视频缺少ep_id，如遇-404错误将尝试从视频详情API获取epid");
                }
                bili_video.get_page_analyzer_with_api_fallback(page_info, ep_id).await
            }
        } => res
    }?;

    // 按需创建保存目录（只在实际下载时创建）
    ensure_parent_dir_for_file(page_path).await?;

    // UnifiedDownloader会自动选择最佳下载方式

    // 获取用户配置的筛选选项
    let config = crate::config::reload_config();
    let filter_option = &config.filter_option;

    // 简化的配置调试日志
    debug!("=== 视频下载配置 ===");
    debug!("视频: {} ({})", video_model.name, video_model.bvid);
    debug!("分页: {} (cid: {})", page_info.name, page_info.cid);
    debug!(
        "质量配置: {} - {} (最高-最低)",
        format!(
            "{:?}({})",
            filter_option.video_max_quality, filter_option.video_max_quality as u32
        ),
        format!(
            "{:?}({})",
            filter_option.video_min_quality, filter_option.video_min_quality as u32
        )
    );
    debug!(
        "音频配置: {} - {} (最高-最低)",
        format!(
            "{:?}({})",
            filter_option.audio_max_quality, filter_option.audio_max_quality as u32
        ),
        format!(
            "{:?}({})",
            filter_option.audio_min_quality, filter_option.audio_min_quality as u32
        )
    );
    debug!("编码偏好: {:?}", filter_option.codecs);

    // 会员状态检查
    let credential = config.credential.load();
    match credential.as_deref() {
        Some(cred) => {
            debug!("用户认证: 已登录 (DedeUserID: {})", cred.dedeuserid);
        }
        None => {
            debug!("用户认证: 未登录 - 高质量视频流可能不可用");
        }
    }

    // 高质量需求提醒
    if filter_option.video_max_quality as u32 >= 120 {
        // 4K及以上
        debug!("⚠️  请求高质量视频(4K+)，需要大会员权限");
    }

    debug!("=== 配置调试结束 ===");

    // 记录开始时间
    let start_time = std::time::Instant::now();

    // 根据流类型进行不同处理
    let best_stream_result = streams.best_stream(filter_option)?;

    // 添加流选择结果日志和质量分析
    debug!("=== 流选择结果 ===");
    match &best_stream_result {
        BestStream::Mixed(stream) => {
            debug!("选择了混合流: {:?}", stream);
        }
        BestStream::VideoAudio { video, audio } => {
            if let VideoStream::DashVideo { quality, codecs, .. } = video {
                let quality_value = *quality as u32;
                let requested_quality = filter_option.video_max_quality as u32;

                debug!("✓ 选择视频流: {} {:?}", quality, codecs);

                // 质量对比分析
                if quality_value < requested_quality {
                    let quality_gap = requested_quality - quality_value;
                    if requested_quality >= 120 && quality_value < 120 {
                        debug!(
                            "⚠️  未获得4K+质量(请求{}，实际{})",
                            filter_option.video_max_quality, quality
                        );
                    } else if quality_gap >= 40 {
                        warn!(
                            "⚠️  视频质量显著低于预期(请求{}，实际{}) - 视频源可能不支持更高质量",
                            filter_option.video_max_quality, quality
                        );
                    } else {
                        info!(
                            "ℹ️  视频质量略低于预期(请求{}，实际{}) - 已选择可用的最高质量",
                            filter_option.video_max_quality, quality
                        );
                    }
                } else {
                    debug!("✓ 获得预期质量或更高");
                }
            }
            if let Some(VideoStream::DashAudio { quality, .. }) = audio {
                debug!("✓ 选择音频流: {:?}({})", quality, *quality as u32);
            } else {
                debug!("ℹ️  无独立音频流(可能为混合流)");
            }
        }
    }
    debug!("=== 流选择结束 ===");

    let total_bytes = match best_stream_result {
        BestStream::Mixed(mix_stream) => {
            let urls = mix_stream.urls();
            download_stream(downloader, &urls, page_path).await?
        }
        BestStream::VideoAudio {
            video: video_stream,
            audio: None,
        } => {
            let urls = video_stream.urls();
            download_stream(downloader, &urls, page_path).await?
        }
        BestStream::VideoAudio {
            video: video_stream,
            audio: Some(audio_stream),
        } => {
            let (tmp_video_path, tmp_audio_path) = (
                page_path.with_extension("tmp_video"),
                page_path.with_extension("tmp_audio"),
            );

            let video_urls = video_stream.urls();
            let video_size = download_stream(downloader, &video_urls, &tmp_video_path)
                .await
                .map_err(|e| {
                    // 使用错误分类器进行统一处理
                    let classified_error = crate::error::ErrorClassifier::classify_error(&e);
                    match classified_error.error_type {
                        crate::error::ErrorType::UserCancelled => {
                            info!("视频流下载因用户暂停而终止");
                        }
                        _ => {
                            error!("视频流下载失败: {:#}", e);
                        }
                    }
                    e
                })?;

            let audio_urls = audio_stream.urls();
            let audio_size = download_stream(downloader, &audio_urls, &tmp_audio_path)
                .await
                .map_err(|e| {
                    // 使用错误分类器进行统一处理
                    let classified_error = crate::error::ErrorClassifier::classify_error(&e);
                    match classified_error.error_type {
                        crate::error::ErrorType::UserCancelled => {
                            info!("音频流下载因用户暂停而终止");
                        }
                        _ => {
                            error!("音频流下载失败: {:#}", e);
                        }
                    }
                    // 异步删除临时视频文件
                    let video_path_clone = tmp_video_path.clone();
                    tokio::spawn(async move {
                        let _ = fs::remove_file(&video_path_clone).await;
                    });
                    e
                })?;

            // 增强的音视频合并，带损坏文件检测和重试机制
            let res = downloader.merge(&tmp_video_path, &tmp_audio_path, page_path).await;

            // 合并失败时的智能处理
            if let Err(e) = res {
                error!("音视频合并失败: {:#}", e);

                // 检查是否是文件损坏导致的失败
                let error_msg = e.to_string();
                if error_msg.contains("Invalid data found when processing input")
                    || error_msg.contains("ffmpeg error")
                    || error_msg.contains("文件损坏")
                {
                    warn!("检测到文件损坏，清理临时文件并标记为重试: {}", error_msg);

                    // 立即清理损坏的临时文件
                    let _ = fs::remove_file(&tmp_video_path).await;
                    let _ = fs::remove_file(&tmp_audio_path).await;

                    // 返回特殊错误，让上层重试下载
                    return Err(anyhow::anyhow!(
                        "视频文件损坏，已清理临时文件，请重试下载: {}",
                        error_msg
                    ));
                } else {
                    // 其他类型的合并错误，清理临时文件后直接返回
                    let _ = fs::remove_file(&tmp_video_path).await;
                    let _ = fs::remove_file(&tmp_audio_path).await;
                    return Err(e);
                }
            }

            // 合并成功，清理临时文件
            let _ = fs::remove_file(tmp_video_path).await;
            let _ = fs::remove_file(tmp_audio_path).await;

            // 获取合并后文件大小，如果失败则使用视频和音频大小之和
            tokio::fs::metadata(page_path)
                .await
                .map(|metadata| metadata.len())
                .unwrap_or(video_size + audio_size)
        }
    };

    // 计算并记录下载速度
    let elapsed = start_time.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();

    if elapsed_secs > 0.0 && total_bytes > 0 {
        let speed_bps = total_bytes as f64 / elapsed_secs;
        let (speed, unit) = if speed_bps >= 1_048_576.0 {
            (speed_bps / 1_048_576.0, "MB/s")
        } else if speed_bps >= 1_024.0 {
            (speed_bps / 1_024.0, "KB/s")
        } else {
            (speed_bps, "B/s")
        };

        info!(
            "视频下载完成，总大小: {:.2} MB，耗时: {:.2} 秒，平均速度: {:.2} {}",
            total_bytes as f64 / 1_048_576.0,
            elapsed_secs,
            speed,
            unit
        );
    }

    Ok(ExecutionStatus::Succeeded)
}

pub async fn fetch_page_danmaku(
    download_danmaku_enabled: bool,
    task_status_requires_run: bool,
    bili_client: &BiliClient,
    video_model: &video::Model,
    page_info: &PageInfo,
    danmaku_path: PathBuf,
    token: CancellationToken,
) -> Result<ExecutionStatus> {
    // 如果弹幕下载已关闭,检查并删除已存在的弹幕文件
    if !download_danmaku_enabled {
        if danmaku_path.exists() {
            if let Err(e) = tokio::fs::remove_file(&danmaku_path).await {
                warn!(
                    "删除弹幕文件失败: 视频「{}」, 路径: {:?}, 错误: {}",
                    &video_model.name, danmaku_path, e
                );
            } else {
                debug!(
                    "弹幕已关闭，已删除弹幕文件: 视频「{}」, 路径: {:?}",
                    &video_model.name, danmaku_path
                );
            }
        } else {
            debug!(
                "弹幕已关闭，无需删除: 视频「{}」, 弹幕文件不存在",
                &video_model.name
            );
        }
        return Ok(ExecutionStatus::Skipped);
    }

    // 弹幕已开启，检查是否需要下载
    if danmaku_path.exists() {
        debug!(
            "弹幕文件已存在，跳过下载: 视频「{}」, 路径: {:?}",
            &video_model.name, danmaku_path
        );
        return Ok(ExecutionStatus::Skipped);
    }

    // 如果任务状态不需要运行(已完成),跳过
    if !task_status_requires_run {
        return Ok(ExecutionStatus::Skipped);
    }

    // 检查 CID 是否有效（-1 表示信息获取失败）
    if page_info.cid < 0 {
        warn!(
            "视频 {} 的 CID 无效（{}），跳过弹幕下载",
            &video_model.name, page_info.cid
        );
        return Ok(ExecutionStatus::Ignored(anyhow::anyhow!("CID 无效，无法下载弹幕")));
    }

    // 检查是否为番剧，如果是番剧则需要从API获取正确的 aid
    let bili_video = if video_model.source_type == Some(1) {
        // 番剧：需要从API获取aid
        if let Some(ep_id) = &video_model.ep_id {
            match tokio::select! {
                biased;
                _ = token.cancelled() => None,
                res = get_bangumi_aid_from_api(bili_client, ep_id, token.clone()) => res,
            } {
                Some(aid) => {
                    debug!("使用番剧API获取到的aid: {}", aid);
                    Video::new_with_aid(bili_client, video_model.bvid.clone(), aid)
                }
                None => {
                    warn!("无法获取番剧 {} (EP{}) 的AID，使用bvid转换", &video_model.name, ep_id);
                    Video::new(bili_client, video_model.bvid.clone())
                }
            }
        } else {
            warn!("番剧 {} 缺少EP ID，使用bvid转换aid", &video_model.name);
            Video::new(bili_client, video_model.bvid.clone())
        }
    } else {
        // 普通视频：使用 bvid 转换的 aid
        Video::new(bili_client, video_model.bvid.clone())
    };

    let danmaku_writer = tokio::select! {
        biased;
        _ = token.cancelled() => return Err(anyhow!("Download cancelled")),
        res = bili_video.get_danmaku_writer(page_info, token.clone()) => res?,
    };

    danmaku_writer.write(danmaku_path).await?;
    Ok(ExecutionStatus::Succeeded)
}

/// 检查视频源是否有弹幕文件需要处理（删除或下载）
/// 当发现缺失的弹幕文件时，会自动重置对应页面和视频的弹幕下载状态
async fn check_danmaku_work_needed(
    video_source: &VideoSourceEnum,
    connection: &DatabaseConnection,
) -> Result<bool> {
    use sea_orm::{EntityTrait, ActiveModelTrait, Set};
    use std::collections::HashSet;

    let download_danmaku_enabled = video_source.download_danmaku();
    let mut has_work = false;
    let mut pages_to_reset = Vec::new();
    let mut videos_to_reset = HashSet::new(); // 记录需要重置的视频ID

    // 查询该视频源的所有视频及其页面
    let videos = entities::video::Entity::find()
        .filter(video_source.filter_expr())
        .all(connection)
        .await?;

    for video_model in videos {
        let pages = entities::page::Entity::find()
            .filter(entities::page::Column::VideoId.eq(video_model.id))
            .all(connection)
            .await?;

        let mut video_has_danmaku_work = false;

        for page_model in pages {
            if let Some(video_path_str) = &page_model.path {
                let video_path = std::path::Path::new(video_path_str);
                if let Some(parent) = video_path.parent() {
                    if let Some(stem) = video_path.file_stem() {
                        let danmaku_path = parent.join(format!("{}.zh-CN.default.ass", stem.to_string_lossy()));

                        if download_danmaku_enabled {
                            // 弹幕开启：检查是否有缺失的弹幕文件
                            if !danmaku_path.exists() {
                                has_work = true;
                                video_has_danmaku_work = true;
                                // 收集需要重置弹幕状态的页面
                                pages_to_reset.push((page_model.id, page_model.video_id, page_model.download_status));
                            }
                        } else {
                            // 弹幕关闭：检查是否有需要删除的弹幕文件
                            if danmaku_path.exists() {
                                has_work = true;
                                video_has_danmaku_work = true;
                                // 收集需要重置弹幕状态的页面
                                pages_to_reset.push((page_model.id, page_model.video_id, page_model.download_status));
                            }
                        }
                    }
                }
            }
        }

        // 如果这个视频有弹幕工作需要做，标记需要重置视频状态
        if video_has_danmaku_work {
            videos_to_reset.insert((video_model.id, video_model.download_status));
        }
    }

    // 如果有需要处理的页面，重置它们的弹幕下载状态
    if !pages_to_reset.is_empty() {
        use crate::utils::status::{PageStatus, VideoStatus};

        let pages_count = pages_to_reset.len();
        info!("发现 {} 个页面的弹幕文件需要处理，正在重置弹幕下载状态...", pages_count);

        // 重置页面状态
        for (page_id, _video_id, download_status) in pages_to_reset {
            let mut page_status = PageStatus::from(download_status);
            // 重置弹幕下载状态（索引3对应弹幕）
            page_status.set(3, 0);

            // 更新数据库
            entities::page::ActiveModel {
                id: Set(page_id),
                download_status: Set(page_status.into()),
                ..Default::default()
            }
            .update(connection)
            .await?;
        }

        info!("已重置 {} 个页面的弹幕下载状态", pages_count);

        // 重置视频状态（清除完成标记，让视频重新进入处理流程）
        let videos_count = videos_to_reset.len();
        if videos_count > 0 {
            info!("正在重置 {} 个视频的下载状态以触发弹幕下载...", videos_count);

            for (video_id, download_status) in videos_to_reset {
                let mut video_status = VideoStatus::from(download_status);
                // 清除完成标记，让视频状态变为未完成
                video_status.set(4, 0); // 索引4对应"分P下载"任务

                entities::video::ActiveModel {
                    id: Set(video_id),
                    download_status: Set(video_status.into()),
                    ..Default::default()
                }
                .update(connection)
                .await?;
            }

            info!("已重置 {} 个视频的下载状态", videos_count);
        }
    }

    Ok(has_work)
}


pub async fn fetch_page_subtitle(
    should_run: bool,
    bili_client: &BiliClient,
    video_model: &video::Model,
    page_info: &PageInfo,
    subtitle_path: &Path,
    token: CancellationToken,
) -> Result<ExecutionStatus> {
    if !should_run {
        return Ok(ExecutionStatus::Skipped);
    }
    let bili_video = Video::new(bili_client, video_model.bvid.clone());
    let subtitles = tokio::select! {
        biased;
        _ = token.cancelled() => return Err(anyhow!("Download cancelled")),
        res = bili_video.get_subtitles(page_info) => res?,
    };
    let tasks = subtitles
        .into_iter()
        .map(|subtitle| async move {
            let path = subtitle_path.with_extension(format!("{}.srt", subtitle.lan));
            ensure_parent_dir_for_file(&path).await.map_err(std::io::Error::other)?;
            tokio::fs::write(path, subtitle.body.to_string()).await
        })
        .collect::<FuturesUnordered<_>>();
    tasks.try_collect::<Vec<()>>().await?;
    Ok(ExecutionStatus::Succeeded)
}

pub async fn generate_page_nfo(
    should_run: bool,
    video_model: &video::Model,
    page_model: &page::Model,
    nfo_path: PathBuf,
    _connection: &DatabaseConnection,
) -> Result<ExecutionStatus> {
    if !should_run {
        return Ok(ExecutionStatus::Skipped);
    }

    let nfo = match video_model.single_page {
        Some(single_page) => {
            if single_page {
                if video_model.collection_id.is_some() {
                    // 合集视频应使用Episode格式，符合Emby标准
                    use crate::utils::nfo::Episode;
                    let mut episode = Episode::from_video_and_page(video_model, page_model);
                    // 对于合集视频，如果数据库中尚未带有 episode_number，按合集顺序编号
                    if video_model.collection_id.is_some() && video_model.episode_number.is_none() {
                        if let Some(col_id) = video_model.collection_id {
                            if let Ok(ep_no) = get_collection_video_episode_number(_connection, col_id, &video_model.bvid).await {
                                episode.episode_number = ep_no;
                            }
                        }
                    }
                    NFO::Episode(episode)
                } else {
                    // 普通单页视频生成Movie
                    use crate::utils::nfo::Movie;
                    NFO::Movie(Movie::from_video_with_pages(video_model, &[page_model.clone()]))
                }
            } else {
                use crate::utils::nfo::Episode;
                let episode = Episode::from_video_and_page(video_model, page_model);
                NFO::Episode(episode)
            }
        }
        None => {
            use crate::utils::nfo::Episode;
            let mut episode = Episode::from_video_and_page(video_model, page_model);
            // 非番剧但属于合集的视频：按合集顺序编号，避免固定为1
            if let Some(col_id) = video_model.collection_id {
                if video_model.episode_number.is_none() {
                    if let Ok(ep_no) = get_collection_video_episode_number(_connection, col_id, &video_model.bvid).await {
                        episode.episode_number = ep_no;
                    }
                }
            }
            NFO::Episode(episode)
        }
    };

    generate_nfo(nfo, nfo_path).await?;
    Ok(ExecutionStatus::Succeeded)
}

async fn get_video_count_for_source(video_source: &VideoSourceEnum, connection: &DatabaseConnection) -> Result<usize> {
    let count = video::Entity::find()
        .filter(video_source.filter_expr())
        .count(connection)
        .await?;
    Ok(count as usize)
}

/// 自动重置风控导致的失败任务
/// 当检测到风控时，将所有失败状态(值为3)、正在进行状态(值为2)以及未完成的任务重置为未开始状态(值为0)
pub async fn auto_reset_risk_control_failures(connection: &DatabaseConnection) -> Result<()> {
    use crate::utils::status::{PageStatus, VideoStatus};
    use bili_sync_entity::{page, video};
    use sea_orm::*;

    info!("检测到风控，开始自动重置失败、进行中和未完成的下载任务...");

    // 查询所有视频和页面数据
    let (all_videos, all_pages) = tokio::try_join!(
        video::Entity::find()
            .select_only()
            .columns([video::Column::Id, video::Column::Name, video::Column::DownloadStatus,])
            .into_tuple::<(i32, String, u32)>()
            .all(connection),
        page::Entity::find()
            .select_only()
            .columns([page::Column::Id, page::Column::Name, page::Column::DownloadStatus,])
            .into_tuple::<(i32, String, u32)>()
            .all(connection)
    )?;

    let mut resetted_videos = 0;
    let mut resetted_pages = 0;

    let txn = connection.begin().await?;

    // 重置视频失败、进行中和未完成状态
    for (id, name, download_status) in all_videos {
        let mut video_status = VideoStatus::from(download_status);
        let mut video_resetted = false;

        // 检查是否为完全成功的状态（所有任务都是1）
        let is_fully_completed = (0..5).all(|task_index| video_status.get(task_index) == 1);

        if !is_fully_completed {
            // 如果不是完全成功，检查所有任务索引，将失败状态(3)、正在进行状态(2)和未开始状态(0)重置为未开始(0)
            for task_index in 0..5 {
                let status_value = video_status.get(task_index);
                if status_value == 3 || status_value == 2 || status_value == 0 {
                    video_status.set(task_index, 0); // 重置为未开始
                    video_resetted = true;
                }
            }
        }

        if video_resetted {
            video::Entity::update(video::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                download_status: sea_orm::Set(video_status.into()),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            resetted_videos += 1;
            debug!("重置视频「{}」的未完成任务状态", name);
        }
    }

    // 重置页面失败、进行中和未完成状态
    for (id, name, download_status) in all_pages {
        let mut page_status = PageStatus::from(download_status);
        let mut page_resetted = false;

        // 检查是否为完全成功的状态（所有任务都是1）
        let is_fully_completed = (0..5).all(|task_index| page_status.get(task_index) == 1);

        if !is_fully_completed {
            // 如果不是完全成功，检查所有任务索引，将失败状态(3)、正在进行状态(2)和未开始状态(0)重置为未开始(0)
            for task_index in 0..5 {
                let status_value = page_status.get(task_index);
                if status_value == 3 || status_value == 2 || status_value == 0 {
                    page_status.set(task_index, 0); // 重置为未开始
                    page_resetted = true;
                }
            }
        }

        if page_resetted {
            page::Entity::update(page::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                download_status: sea_orm::Set(page_status.into()),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            resetted_pages += 1;
            debug!("重置页面「{}」的未完成任务状态", name);
        }
    }

    txn.commit().await?;

    if resetted_videos > 0 || resetted_pages > 0 {
        info!(
            "风控自动重置完成：重置了 {} 个视频和 {} 个页面的未完成任务状态",
            resetted_videos, resetted_pages
        );
    } else {
        info!("风控自动重置完成：所有任务都已完成，无需重置");
    }

    Ok(())
}

/// 获取合集中视频的集数序号
/// 根据视频在合集中的发布时间顺序确定集数
async fn get_collection_video_episode_number(
    connection: &DatabaseConnection,
    collection_id: i32,
    bvid: &str,
) -> Result<i32> {
    use bili_sync_entity::video;
    use sea_orm::*;

    // 获取该合集中所有视频，按发布时间排序
    let videos = video::Entity::find()
        .filter(video::Column::CollectionId.eq(collection_id))
        .order_by_asc(video::Column::Pubtime)
        .select_only()
        .columns([video::Column::Bvid, video::Column::Pubtime])
        .into_tuple::<(String, chrono::NaiveDateTime)>()
        .all(connection)
        .await?;

    // 找到当前视频的位置，返回序号（从1开始）
    for (index, (video_bvid, _)) in videos.iter().enumerate() {
        if video_bvid == bvid {
            return Ok((index + 1) as i32);
        }
    }

    // 如果没找到，返回错误
    Err(anyhow!("视频 {} 在合集 {} 中未找到", bvid, collection_id))
}

/// 修复page表中错误的video_id
///
/// **注意**：在写穿透模式下，此功能理论上不应该需要，因为所有写操作都直接写入主数据库，
/// 确保了ID的一致性。但为了兼容可能存在的历史数据问题，仍然保留此功能。
/// 使用两阶段策略避免唯一约束冲突
pub async fn fix_page_video_ids(connection: &DatabaseConnection) -> Result<()> {
    debug!("开始检查并修复page表的video_id和cid不匹配问题");
    warn!("注意：在写穿透模式下，此数据修复功能理论上不应该需要。如果频繁出现需要修复的数据，请检查系统配置。");

    // 使用事务确保原子性
    let txn = connection.begin().await?;

    // 1. 首先处理cid不匹配的记录 - 这些应该删除
    let cid_mismatch_count: i64 = txn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            r#"
            SELECT COUNT(*) as count 
            FROM page p 
            JOIN video v ON p.video_id = v.id 
            WHERE p.pid = 1 
            AND v.cid IS NOT NULL 
            AND p.cid != v.cid
            "#,
            vec![],
        ))
        .await?
        .and_then(|row| row.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0);

    // 创建临时表来跟踪需要设置auto_download=0的video
    let mut videos_to_disable = Vec::new();

    if cid_mismatch_count > 0 {
        warn!(
            "发现 {} 条cid不匹配的page记录，这些记录的内容已变化，将删除",
            cid_mismatch_count
        );

        // 先收集这些记录对应的video_id（用于后续设置auto_download=0）
        let mismatch_videos = txn
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                r#"
                SELECT DISTINCT p.video_id
                FROM page p 
                JOIN video v ON p.video_id = v.id 
                WHERE p.pid = 1 
                AND v.cid IS NOT NULL 
                AND p.cid != v.cid
                "#,
                vec![],
            ))
            .await?;

        for row in mismatch_videos {
            if let Ok(video_id) = row.try_get_by_index::<i64>(0) {
                videos_to_disable.push(video_id);
            }
        }

        // 删除cid不匹配的page记录
        let delete_mismatch_result = txn
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                r#"
                DELETE FROM page 
                WHERE id IN (
                    SELECT p.id 
                    FROM page p 
                    JOIN video v ON p.video_id = v.id 
                    WHERE p.pid = 1 
                    AND v.cid IS NOT NULL 
                    AND p.cid != v.cid
                )
                "#,
                vec![],
            ))
            .await?;

        info!(
            "已删除 {} 条cid不匹配的page记录",
            delete_mismatch_result.rows_affected()
        );
    }

    // 2. 然后统计有多少page记录需要修复video_id
    let wrong_pages_count: i64 = txn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            r#"
            SELECT COUNT(*) as count 
            FROM page p 
            LEFT JOIN video v ON p.video_id = v.id 
            WHERE v.id IS NULL
            "#,
            vec![],
        ))
        .await?
        .and_then(|row| row.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0);

    if wrong_pages_count == 0 {
        debug!("所有page记录的video_id都正确，无需修复");
        txn.commit().await?;
        return Ok(());
    }

    info!("发现 {} 条page记录需要修复video_id", wrong_pages_count);

    // 3. 第一阶段：将错误的video_id设置为临时的负数值
    info!("第一阶段：设置临时video_id避免冲突...");
    let set_temp_id_sql = r#"
        UPDATE page
        SET video_id = -id
        WHERE video_id NOT IN (SELECT id FROM video)
    "#;

    let phase1_result = txn
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            set_temp_id_sql,
            vec![],
        ))
        .await?;

    info!("已将 {} 条记录设置为临时video_id", phase1_result.rows_affected());

    // 4. 第二阶段：根据cid匹配更新为正确的video_id
    info!("第二阶段：更新为正确的video_id...");

    // 4.1 修复单P视频（pid=1）- 分批处理避免冲突
    info!("开始修复单P视频...");

    // 先修复那些不会产生冲突的记录
    let fix_no_conflict_sql = r#"
        UPDATE page
        SET video_id = (
            SELECT v.id 
            FROM video v 
            WHERE v.cid = page.cid
            LIMIT 1
        )
        WHERE video_id < 0
        AND pid = 1
        AND EXISTS (SELECT 1 FROM video v WHERE v.cid = page.cid)
        AND NOT EXISTS (
            SELECT 1 FROM page p2 
            WHERE p2.video_id = (SELECT v.id FROM video v WHERE v.cid = page.cid LIMIT 1)
            AND p2.pid = 1
        )
    "#;

    let no_conflict_result = txn
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            fix_no_conflict_sql,
            vec![],
        ))
        .await?;

    info!("修复了 {} 条不冲突的单P视频记录", no_conflict_result.rows_affected());

    // 处理会冲突的记录 - 这些需要特殊处理
    let conflicting_pages = txn
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            r#"
            SELECT p1.id, p1.cid, v.id as correct_video_id, p2.id as existing_id, p2.cid as existing_cid
            FROM page p1
            JOIN video v ON p1.cid = v.cid
            LEFT JOIN page p2 ON v.id = p2.video_id AND p2.pid = 1
            WHERE p1.video_id < 0 AND p1.pid = 1 AND p2.id IS NOT NULL
            "#,
            vec![],
        ))
        .await?;

    let mut conflict_count = 0u64;
    let mut duplicate_deleted = 0u64;

    for row in conflicting_pages {
        if let (Ok(page_id), Ok(page_cid), Ok(_correct_vid), Ok(existing_id), Ok(existing_cid)) = (
            row.try_get_by_index::<i32>(0),
            row.try_get_by_index::<i64>(1),
            row.try_get_by_index::<i32>(2),
            row.try_get_by_index::<i32>(3),
            row.try_get_by_index::<i64>(4),
        ) {
            // 只有当两个page的cid相同时，才是真正的重复记录
            if page_cid == existing_cid {
                // 真正的重复，删除临时ID的那条
                txn.execute(Statement::from_sql_and_values(
                    DatabaseBackend::Sqlite,
                    r#"DELETE FROM page WHERE id = ?"#,
                    vec![page_id.into()],
                ))
                .await?;
                duplicate_deleted += 1;
                debug!("删除重复的page记录 id={} (与id={}重复)", page_id, existing_id);
            } else {
                // 不同的cid，说明是不同的视频，记录错误但不处理
                conflict_count += 1;
                warn!(
                    "发现冲突记录：page.id={} cid={} 与 page.id={} cid={} 冲突，需要手动处理",
                    page_id, page_cid, existing_id, existing_cid
                );
            }
        }
    }

    info!(
        "修复单P视频完成：删除了 {} 条真正的重复记录，发现 {} 条需要手动处理的冲突",
        duplicate_deleted, conflict_count
    );

    // 4.2 修复多P视频（pid>1）
    info!("修复多P视频的video_id...");

    // 使用路径匹配方式修复多P视频
    // 原理：同一视频的多个分P在同一目录下，通过找到同目录的pid=1记录来获取正确的video_id
    let fix_multi_p_sql = r#"
        UPDATE page
        SET video_id = (
            SELECT v.id 
            FROM page p1
            JOIN video v ON v.cid = p1.cid
            WHERE p1.pid = 1 
            -- 使用RTRIM去除文件名，只保留目录路径进行匹配
            AND RTRIM(p1.path, REPLACE(p1.path, '/', '')) = RTRIM(page.path, REPLACE(page.path, '/', ''))
            LIMIT 1
        )
        WHERE video_id < 0 
        AND pid > 1
        AND EXISTS (
            SELECT 1 FROM page p1
            JOIN video v ON v.cid = p1.cid
            WHERE p1.pid = 1 
            AND RTRIM(p1.path, REPLACE(p1.path, '/', '')) = RTRIM(page.path, REPLACE(page.path, '/', ''))
        )
    "#;

    let multi_p_result = txn
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            fix_multi_p_sql,
            vec![],
        ))
        .await?;

    info!("修复了 {} 条多P视频的page记录", multi_p_result.rows_affected());

    // 5. 处理无法修复的记录（video_id仍为负数的）
    let orphan_count: i64 = txn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            r#"
            SELECT COUNT(*) as count 
            FROM page 
            WHERE video_id < 0
            "#,
            vec![],
        ))
        .await?
        .and_then(|row| row.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0);

    if orphan_count > 0 {
        warn!(
            "发现 {} 条无法修复的page记录（找不到对应的video），将删除这些孤立记录",
            orphan_count
        );

        // 删除无法修复的孤立记录
        let delete_result = txn
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                r#"DELETE FROM page WHERE video_id < 0"#,
                vec![],
            ))
            .await?;

        info!("已删除 {} 条孤立的page记录", delete_result.rows_affected());
    }

    // 6. 设置cid不匹配的video的deleted为1
    // 但要排除已修复的video（即在修复过程中成功更新的video）
    if !videos_to_disable.is_empty() {
        info!("标记 {} 个cid不匹配video为已删除", videos_to_disable.len());

        // 收集所有已修复的video_id（这些不应该被标记为已删除）
        let fixed_videos = txn
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                r#"
                SELECT DISTINCT video_id 
                FROM page 
                WHERE video_id > 0
                "#,
                vec![],
            ))
            .await?;

        let mut fixed_video_ids = std::collections::HashSet::new();
        for row in fixed_videos {
            if let Ok(video_id) = row.try_get_by_index::<i64>(0) {
                fixed_video_ids.insert(video_id);
            }
        }

        // 只设置那些不在fixed_video_ids中的video
        let mut disabled_count = 0;
        for video_id in &videos_to_disable {
            // 如果这个video已经被修复（有正确的page记录），则跳过
            if fixed_video_ids.contains(video_id) {
                continue;
            }

            let update_result = txn
                .execute(Statement::from_sql_and_values(
                    DatabaseBackend::Sqlite,
                    r#"UPDATE video SET deleted = 1 WHERE id = ?"#,
                    vec![(*video_id).into()],
                ))
                .await?;

            if update_result.rows_affected() > 0 {
                disabled_count += 1;
            }
        }

        info!("已标记 {} 个video为已删除（排除了已修复的记录）", disabled_count);

        // 6.5 自动为涉及的源启用 scan_deleted_videos
        if disabled_count > 0 {
            info!("检测到视频被标记为已删除，正在自动启用相关源的'扫描已删除视频'功能...");

            // 查询刚刚被标记为已删除的视频的源信息
            let deleted_videos_sources = txn
                .query_all(Statement::from_sql_and_values(
                    DatabaseBackend::Sqlite,
                    r#"
                    SELECT DISTINCT 
                        submission_id,
                        collection_id,
                        favorite_id,
                        watch_later_id,
                        source_id,
                        source_type
                    FROM video 
                    WHERE deleted = 1 
                    AND id IN (SELECT value FROM json_each(?))
                    "#,
                    vec![serde_json::to_string(&videos_to_disable)?.into()],
                ))
                .await?;

            // 收集各类型源的ID
            let mut submission_ids = std::collections::HashSet::new();
            let mut collection_ids = std::collections::HashSet::new();
            let mut favorite_ids = std::collections::HashSet::new();
            let mut watch_later_ids = std::collections::HashSet::new();
            let mut bangumi_source_ids = std::collections::HashSet::new();

            for row in deleted_videos_sources {
                if let Ok(Some(id)) = row.try_get::<Option<i32>>("", "submission_id") {
                    submission_ids.insert(id);
                }
                if let Ok(Some(id)) = row.try_get::<Option<i32>>("", "collection_id") {
                    collection_ids.insert(id);
                }
                if let Ok(Some(id)) = row.try_get::<Option<i32>>("", "favorite_id") {
                    favorite_ids.insert(id);
                }
                if let Ok(Some(id)) = row.try_get::<Option<i32>>("", "watch_later_id") {
                    watch_later_ids.insert(id);
                }
                // 番剧通过source_id和source_type=1判断
                if let (Ok(Some(source_id)), Ok(Some(source_type))) = (
                    row.try_get::<Option<i32>>("", "source_id"),
                    row.try_get::<Option<i32>>("", "source_type"),
                ) {
                    if source_type == 1 {
                        bangumi_source_ids.insert(source_id);
                    }
                }
            }

            // 批量更新各个源表的scan_deleted_videos字段
            let mut enabled_sources = vec![];

            // UP主投稿
            if !submission_ids.is_empty() {
                let placeholders = submission_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let result = txn
                    .execute(Statement::from_sql_and_values(
                        DatabaseBackend::Sqlite,
                        format!(
                            "UPDATE submission SET scan_deleted_videos = 1 
                             WHERE id IN ({}) AND scan_deleted_videos = 0",
                            placeholders
                        ),
                        submission_ids.iter().map(|id| (*id).into()).collect::<Vec<_>>(),
                    ))
                    .await?;
                if result.rows_affected() > 0 {
                    enabled_sources.push(format!("{}个UP主投稿", result.rows_affected()));
                }
            }

            // 合集
            if !collection_ids.is_empty() {
                let placeholders = collection_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let result = txn
                    .execute(Statement::from_sql_and_values(
                        DatabaseBackend::Sqlite,
                        format!(
                            "UPDATE collection SET scan_deleted_videos = 1 
                             WHERE id IN ({}) AND scan_deleted_videos = 0",
                            placeholders
                        ),
                        collection_ids.iter().map(|id| (*id).into()).collect::<Vec<_>>(),
                    ))
                    .await?;
                if result.rows_affected() > 0 {
                    enabled_sources.push(format!("{}个合集", result.rows_affected()));
                }
            }

            // 收藏夹
            if !favorite_ids.is_empty() {
                let placeholders = favorite_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let result = txn
                    .execute(Statement::from_sql_and_values(
                        DatabaseBackend::Sqlite,
                        format!(
                            "UPDATE favorite SET scan_deleted_videos = 1 
                             WHERE id IN ({}) AND scan_deleted_videos = 0",
                            placeholders
                        ),
                        favorite_ids.iter().map(|id| (*id).into()).collect::<Vec<_>>(),
                    ))
                    .await?;
                if result.rows_affected() > 0 {
                    enabled_sources.push(format!("{}个收藏夹", result.rows_affected()));
                }
            }

            // 稍后再看
            if !watch_later_ids.is_empty() {
                let placeholders = watch_later_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let result = txn
                    .execute(Statement::from_sql_and_values(
                        DatabaseBackend::Sqlite,
                        format!(
                            "UPDATE watch_later SET scan_deleted_videos = 1 
                             WHERE id IN ({}) AND scan_deleted_videos = 0",
                            placeholders
                        ),
                        watch_later_ids.iter().map(|id| (*id).into()).collect::<Vec<_>>(),
                    ))
                    .await?;
                if result.rows_affected() > 0 {
                    enabled_sources.push(format!("{}个稍后再看", result.rows_affected()));
                }
            }

            // 番剧
            if !bangumi_source_ids.is_empty() {
                let placeholders = bangumi_source_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let result = txn
                    .execute(Statement::from_sql_and_values(
                        DatabaseBackend::Sqlite,
                        format!(
                            "UPDATE video_source SET scan_deleted_videos = 1 
                             WHERE id IN ({}) AND scan_deleted_videos = 0",
                            placeholders
                        ),
                        bangumi_source_ids.iter().map(|id| (*id).into()).collect::<Vec<_>>(),
                    ))
                    .await?;
                if result.rows_affected() > 0 {
                    enabled_sources.push(format!("{}个番剧", result.rows_affected()));
                }
            }

            if !enabled_sources.is_empty() {
                info!(
                    "已自动启用以下视频源的'扫描已删除视频'功能: {}",
                    enabled_sources.join(", ")
                );
            }
        }
    }

    // 7. 提交事务
    txn.commit().await?;

    // 8. 最终验证
    let final_check: i64 = connection
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            r#"
            SELECT COUNT(*) as count 
            FROM page p 
            LEFT JOIN video v ON p.video_id = v.id 
            WHERE v.id IS NULL
            "#,
            vec![],
        ))
        .await?
        .and_then(|row| row.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0);

    if final_check == 0 {
        info!("所有page记录的video_id修复完成！");
    } else {
        error!("修复后仍有 {} 条page记录的video_id错误，请检查", final_check);
    }

    Ok(())
}

/// 填充数据库中所有缺失cid的视频
/// 这个函数在迁移完成后运行，用于批量获取并填充视频的cid
pub async fn populate_missing_video_cids(
    bili_client: &BiliClient,
    connection: &DatabaseConnection,
    token: CancellationToken,
) -> Result<()> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    debug!("开始检查并填充缺失的视频cid");

    // 查询所有cid为空的视频
    let videos_without_cid = video::Entity::find()
        .filter(video::Column::Cid.is_null())
        .filter(video::Column::Valid.eq(true))
        .filter(video::Column::Deleted.eq(0))
        .all(connection)
        .await?;

    if videos_without_cid.is_empty() {
        debug!("所有视频都已有cid，无需填充");
        return Ok(());
    }

    info!("发现 {} 个视频需要填充cid", videos_without_cid.len());

    // 批量处理视频，每批10个
    let chunk_size = 10;
    let total_batches = videos_without_cid.len().div_ceil(chunk_size);

    for (batch_idx, chunk) in videos_without_cid.chunks(chunk_size).enumerate() {
        if token.is_cancelled() {
            info!("cid填充任务被取消");
            return Ok(());
        }

        info!("处理第 {}/{} 批视频", batch_idx + 1, total_batches);

        let futures = chunk.iter().map(|video_model| {
            let bili_client = bili_client.clone();
            let connection = connection.clone();
            let token = token.clone();
            let video_model = video_model.clone();

            async move {
                // 获取视频详情
                let video = Video::new(&bili_client, video_model.bvid.clone());

                let view_info = tokio::select! {
                    biased;
                    _ = token.cancelled() => return Err(anyhow!("任务被取消")),
                    res = video.get_view_info() => res,
                };

                match view_info {
                    Ok(VideoInfo::Detail { pages, .. }) => {
                        // 获取第一个page的cid
                        if let Some(first_page) = pages.first() {
                            let bvid = video_model.bvid.clone();
                            let cid = first_page.cid;
                            let mut video_active_model: video::ActiveModel = video_model.into();
                            video_active_model.cid = Set(Some(cid));
                            video_active_model.save(&connection).await?;

                            // 触发异步同步到内存DB

                            debug!("成功更新视频 {} 的cid: {}", bvid, cid);
                        }
                    }
                    Err(e) => {
                        warn!("获取视频 {} 详情失败，跳过cid填充: {}", video_model.bvid, e);
                    }
                    _ => {
                        warn!("视频 {} 返回了非预期的信息类型", video_model.bvid);
                    }
                }

                Ok::<_, anyhow::Error>(())
            }
        });

        let results: Vec<_> = futures::future::join_all(futures).await;

        for result in results {
            if let Err(e) = result {
                error!("处理视频时出错: {}", e);
            }
        }

        // 批次之间添加延迟，避免触发风控
        if batch_idx < total_batches - 1 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    info!("cid填充任务完成");
    Ok(())
}

/// 检查文件夹是否为同一视频的文件夹
fn is_same_video_folder(folder_path: &std::path::Path, video_model: &video::Model) -> bool {
    use std::fs;

    if !folder_path.exists() {
        debug!("文件夹不存在: {:?}", folder_path);
        return false;
    }

    debug!("=== 智能冲突检测开始 ===");
    debug!("检查文件夹: {:?}", folder_path);
    debug!("数据库存储路径: {}", video_model.path);
    debug!("视频BVID: {}", video_model.bvid);
    debug!("视频标题: {}", video_model.name);

    // 方法1：增强的数据库路径匹配
    let db_path = std::path::Path::new(&video_model.path);

    // 1.1 完整路径匹配
    if folder_path == db_path {
        debug!("✓ 通过完整路径匹配确认为同一视频文件夹");
        return true;
    }

    // 1.2 规范化路径比较（处理不同的路径分隔符）
    let folder_normalized = folder_path.to_string_lossy().replace('\\', "/");
    let db_normalized = db_path.to_string_lossy().replace('\\', "/");
    if folder_normalized == db_normalized {
        debug!("✓ 通过规范化路径匹配确认为同一视频文件夹");
        return true;
    }

    // 1.3 文件夹名称匹配（原有逻辑）
    if let Some(db_folder_name) = db_path.file_name() {
        if let Some(check_folder_name) = folder_path.file_name() {
            if db_folder_name == check_folder_name {
                debug!("✓ 通过文件夹名称匹配确认为同一视频文件夹");
                return true;
            }
        }
    }

    // 1.4 相对路径后缀匹配
    if let Some(db_folder_name) = db_path.file_name() {
        if folder_path.ends_with(db_folder_name) {
            debug!("✓ 通过路径后缀匹配确认为同一视频文件夹");
            return true;
        }
    }

    debug!("⚠ 数据库路径匹配失败，尝试文件内容检测");

    // 方法2：扩展的文件内容检测
    if let Ok(entries) = fs::read_dir(folder_path) {
        let mut found_media_files = false;
        let mut found_bvid_files = false;
        let mut found_title_files = false;

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy().to_lowercase();

            // 2.1 检查包含BVID的文件（扩展文件类型）
            if file_name_str.contains(&video_model.bvid.to_lowercase()) {
                if file_name_str.ends_with(".tmp_video")
                    || file_name_str.ends_with(".tmp_audio")
                    || file_name_str.ends_with(".mp4")
                    || file_name_str.ends_with(".mkv")
                    || file_name_str.ends_with(".flv")
                    || file_name_str.ends_with(".webm")
                    || file_name_str.ends_with(".nfo")
                    || file_name_str.ends_with(".jpg")
                    || file_name_str.ends_with(".png")
                    || file_name_str.ends_with(".ass")
                    || file_name_str.ends_with(".srt")
                {
                    debug!(
                        "✓ 通过BVID文件匹配确认为同一视频文件夹: {} (匹配文件: {})",
                        folder_path.display(),
                        file_name_str
                    );
                    return true;
                }
                found_bvid_files = true;
            }

            // 2.2 检查包含视频标题关键词的文件
            let video_title_clean = video_model
                .name
                .to_lowercase()
                .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "");
            if !video_title_clean.is_empty() && video_title_clean.len() > 3 {
                // 提取标题的前几个字符作为关键词
                let title_keywords: Vec<&str> = video_title_clean.split_whitespace().take(3).collect();
                for keyword in title_keywords {
                    if keyword.len() > 2 && file_name_str.contains(keyword) {
                        if file_name_str.ends_with(".mp4")
                            || file_name_str.ends_with(".mkv")
                            || file_name_str.ends_with(".flv")
                            || file_name_str.ends_with(".webm")
                            || file_name_str.ends_with(".nfo")
                        {
                            debug!(
                                "✓ 通过标题关键词匹配确认为同一视频文件夹: {} (匹配关键词: {}, 文件: {})",
                                folder_path.display(),
                                keyword,
                                file_name_str
                            );
                            return true;
                        }
                        found_title_files = true;
                    }
                }
            }

            // 2.3 检查是否有媒体相关文件（降低要求）
            if file_name_str.ends_with(".mp4")
                || file_name_str.ends_with(".mkv")
                || file_name_str.ends_with(".flv")
                || file_name_str.ends_with(".webm")
                || file_name_str.ends_with(".nfo")
                || file_name_str.ends_with(".jpg")
                || file_name_str.ends_with(".png")
                || file_name_str.ends_with(".ass")
                || file_name_str.ends_with(".srt")
            {
                found_media_files = true;
            }
        }

        debug!(
            "文件检测结果: 媒体文件={}, BVID文件={}, 标题文件={}",
            found_media_files, found_bvid_files, found_title_files
        );

        // 如果找到了相关文件但没有精确匹配，记录为可疑
        if found_media_files || found_bvid_files || found_title_files {
            debug!("⚠ 文件夹包含相关文件但无法确认为同一视频文件夹: {:?}", folder_path);
        }
    }

    debug!("✗ 无法确认为同一视频文件夹: {:?}", folder_path);
    debug!("=== 智能冲突检测结束 ===");
    false
}

/// 生成唯一的文件夹名称，避免同名冲突（增强版）
pub fn generate_unique_folder_name(
    parent_dir: &std::path::Path,
    base_name: &str,
    video_model: &video::Model,
    pubtime: &str,
) -> String {
    let mut unique_name = base_name.to_string();
    let mut counter = 0;

    // 检查基础名称是否已存在
    let base_path = parent_dir.join(&unique_name);
    if !base_path.exists() {
        return unique_name;
    }

    // 如果存在，智能检查这个文件夹是否就是当前视频的文件夹
    if is_same_video_folder(&base_path, video_model) {
        debug!("文件夹 {:?} 已是当前视频的文件夹，无需生成新名称", base_path);
        return unique_name;
    }

    // 确认是真正的冲突，开始生成唯一名称
    debug!("检测到真实的文件夹名冲突，开始生成唯一名称: {}", base_name);

    // 如果存在真正冲突，先尝试追加发布时间
    unique_name = format!("{}-{}", base_name, pubtime);
    let time_path = parent_dir.join(&unique_name);
    if !time_path.exists() {
        info!("检测到下载文件夹名冲突，追加发布时间: {} -> {}", base_name, unique_name);
        return unique_name;
    }

    // 如果发布时间也冲突，追加BVID
    unique_name = format!("{}-{}", base_name, &video_model.bvid);
    let bvid_path = parent_dir.join(&unique_name);
    if !bvid_path.exists() {
        info!("检测到下载文件夹名冲突，追加BVID: {} -> {}", base_name, unique_name);
        return unique_name;
    }

    // 如果都冲突，使用数字后缀
    loop {
        counter += 1;
        unique_name = format!("{}-{}", base_name, counter);
        let numbered_path = parent_dir.join(&unique_name);
        if !numbered_path.exists() {
            warn!(
                "检测到严重下载文件夹名冲突，使用数字后缀: {} -> {}",
                base_name, unique_name
            );
            return unique_name;
        }

        // 防止无限循环，使用随机后缀
        if counter > 1000 {
            warn!("下载文件夹名冲突解决失败，使用随机后缀");
            let random_suffix: u32 = rand::random::<u32>() % 90000 + 10000;
            unique_name = format!("{}-{}", base_name, random_suffix);
            return unique_name;
        }
    }
}

#[cfg(test)]
mod tests {
    use handlebars::handlebars_helper;
    use serde_json::json;

    use crate::config::PathSafeTemplate;

    #[test]
    fn test_template_usage() {
        let mut template = handlebars::Handlebars::new();
        handlebars_helper!(truncate: |s: String, len: usize| {
            if s.chars().count() > len {
                s.chars().take(len).collect::<String>()
            } else {
                s.to_string()
            }
        });
        template.register_helper("truncate", Box::new(truncate));
        let _ = template.path_safe_register("video", "test{{bvid}}test");
        let _ = template.path_safe_register("test_truncate", "哈哈，{{ truncate title 30 }}");
        let _ = template.path_safe_register("test_path_unix", "{{ truncate title 7 }}/test/a");
        let _ = template.path_safe_register("test_path_windows", r"{{ truncate title 7 }}\\test\\a");
        #[cfg(not(windows))]
        {
            assert_eq!(
                template
                    .path_safe_render("test_path_unix", &json!({"title": "关注/永雏塔菲喵"}))
                    .unwrap(),
                "关注_永雏塔菲/test/a"
            );
            assert_eq!(
                template
                    .path_safe_render("test_path_windows", &json!({"title": "关注/永雏塔菲喵"}))
                    .unwrap(),
                "关注_永雏塔菲_test_a"
            );
        }
        #[cfg(windows)]
        {
            assert_eq!(
                template
                    .path_safe_render("test_path_unix", &json!({"title": "关注/永雏塔菲喵"}))
                    .unwrap(),
                "关注_永雏塔菲_test_a"
            );
            assert_eq!(
                template
                    .path_safe_render("test_path_windows", &json!({"title": "关注/永雏塔菲喵"}))
                    .unwrap(),
                "关注_永雏塔菲\\test\\a"
            );
        }
        assert_eq!(
            template
                .path_safe_render("video", &json!({"bvid": "BV1b5411h7g7"}))
                .unwrap(),
            "testBV1b5411h7g7test"
        );
        assert_eq!(
            template
                .path_safe_render(
                    "test_truncate",
                    &json!({"title": "你说得对，但是 Rust 是由 Mozilla 自主研发的一款全新的编译期格斗游戏。\
                    编译将发生在一个被称作「Cargo」的构建系统中。在这里，被引用的指针将被授予「生命周期」之力，导引对象安全。\
                    你将扮演一位名为「Rustacean」的神秘角色, 在与「Rustc」的搏斗中邂逅各种骨骼惊奇的傲娇报错。\
                    征服她们、通过编译同时，逐步发掘「C++」程序崩溃的真相。"})
                )
                .unwrap(),
            "哈哈，你说得对，但是 Rust 是由 Mozilla 自主研发的一"
        );
    }

    // 旧的87007/87008错误检测测试已清理，现在使用革命性的upower字段检测
}
