use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use axum::extract::{Extension, Path, Query};

use bili_sync_entity::*;
use bili_sync_migration::Expr;
use chrono::Utc;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait, Unchanged,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use utoipa::OpenApi;

use crate::api::auth::OpenAPIAuth;
use crate::api::error::InnerApiError;
use crate::api::request::{
    AddVideoSourceRequest, BatchUpdateConfigRequest, ConfigHistoryRequest, ResetSpecificTasksRequest,
    ResetVideoSourcePathRequest, SetupAuthTokenRequest, UpdateConfigItemRequest, UpdateConfigRequest,
    UpdateCredentialRequest, UpdateVideoStatusRequest, VideosRequest,
};
use crate::api::response::{
    AddVideoSourceResponse, BangumiSeasonInfo, ConfigChangeInfo, ConfigHistoryResponse, ConfigItemResponse,
    ConfigReloadResponse, ConfigResponse, ConfigValidationResponse, DeleteVideoResponse, DeleteVideoSourceResponse,
    HotReloadStatusResponse, InitialSetupCheckResponse, PageInfo, ResetAllVideosResponse, ResetVideoResponse,
    ResetVideoSourcePathResponse, SetupAuthTokenResponse, UpdateConfigResponse, UpdateCredentialResponse,
    UpdateVideoStatusResponse, VideoInfo, VideoResponse, VideoSource, VideoSourcesResponse, VideosResponse,
};
use crate::api::wrapper::{ApiError, ApiResponse};
use crate::utils::nfo::NFO;
use crate::utils::status::{PageStatus, VideoStatus};

/// 标准化文件路径格式
fn normalize_file_path(path: &str) -> String {
    // 将所有反斜杠转换为正斜杠，保持路径一致性
    path.replace('\\', "/")
}

#[derive(OpenApi)]
#[openapi(
    paths(get_video_sources, get_videos, get_video, reset_video, reset_all_videos, reset_specific_tasks, update_video_status, add_video_source, update_video_source_enabled, update_video_source_scan_deleted, reset_video_source_path, delete_video_source, reload_config, get_config, update_config, get_bangumi_seasons, search_bilibili, get_user_favorites, get_user_collections, get_user_followings, get_subscribed_collections, get_logs, get_queue_status, proxy_image, get_config_item, get_config_history, validate_config, get_hot_reload_status, check_initial_setup, setup_auth_token, update_credential, pause_scanning_endpoint, resume_scanning_endpoint, get_task_control_status, get_video_play_info, proxy_video_stream, validate_favorite, get_user_favorites_by_uid),
    modifiers(&OpenAPIAuth),
    security(
        ("Token" = []),
    )
)]
pub struct ApiDoc;

/// 移除配置文件路径获取 - 配置现在完全基于数据库
#[allow(dead_code)]
fn get_config_path() -> Result<PathBuf> {
    // 配置现在完全基于数据库，不再使用配置文件
    dirs::config_dir()
        .context("无法获取配置目录")
        .map(|dir| dir.join("bili-sync").join("config.toml"))
}

/// 列出所有视频来源
#[utoipa::path(
    get,
    path = "/api/video-sources",
    responses(
        (status = 200, body = ApiResponse<VideoSourcesResponse>),
    )
)]
pub async fn get_video_sources(
    Extension(db): Extension<Arc<DatabaseConnection>>,
) -> Result<ApiResponse<VideoSourcesResponse>, ApiError> {
    // 获取各类视频源
    let collection_sources = collection::Entity::find()
        .select_only()
        .columns([
            collection::Column::Id,
            collection::Column::Name,
            collection::Column::Enabled,
            collection::Column::Path,
            collection::Column::ScanDeletedVideos,
        ])
        .into_model::<VideoSource>()
        .all(db.as_ref())
        .await?;

    let favorite_sources = favorite::Entity::find()
        .select_only()
        .columns([
            favorite::Column::Id,
            favorite::Column::Name,
            favorite::Column::Enabled,
            favorite::Column::Path,
            favorite::Column::ScanDeletedVideos,
        ])
        .into_model::<VideoSource>()
        .all(db.as_ref())
        .await?;

    let submission_sources = submission::Entity::find()
        .select_only()
        .columns([
            submission::Column::Id,
            submission::Column::Enabled,
            submission::Column::Path,
            submission::Column::ScanDeletedVideos,
        ])
        .column_as(submission::Column::UpperName, "name")
        .into_model::<VideoSource>()
        .all(db.as_ref())
        .await?;

    let watch_later_sources = watch_later::Entity::find()
        .select_only()
        .columns([
            watch_later::Column::Id,
            watch_later::Column::Enabled,
            watch_later::Column::Path,
            watch_later::Column::ScanDeletedVideos,
        ])
        .column_as(Expr::value("稍后再看"), "name")
        .into_model::<VideoSource>()
        .all(db.as_ref())
        .await?;

    // 确保bangumi_sources是一个数组，即使为空
    let bangumi_sources = video_source::Entity::find()
        .filter(video_source::Column::Type.eq(1))
        .select_only()
        .columns([
            video_source::Column::Id,
            video_source::Column::Name,
            video_source::Column::Enabled,
            video_source::Column::Path,
            video_source::Column::ScanDeletedVideos,
        ])
        .into_model::<VideoSource>()
        .all(db.as_ref())
        .await?;

    // 返回响应，确保每个分类都是一个数组
    Ok(ApiResponse::ok(VideoSourcesResponse {
        collection: collection_sources,
        favorite: favorite_sources,
        submission: submission_sources,
        watch_later: watch_later_sources,
        bangumi: bangumi_sources,
    }))
}

/// 列出视频的基本信息，支持根据视频来源筛选、名称查找和分页
#[utoipa::path(
    get,
    path = "/api/videos",
    params(
        VideosRequest,
    ),
    responses(
        (status = 200, body = ApiResponse<VideosResponse>),
    )
)]
pub async fn get_videos(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    Query(params): Query<VideosRequest>,
) -> Result<ApiResponse<VideosResponse>, ApiError> {
    let mut query = video::Entity::find();

    // 根据配置决定是否过滤已删除的视频
    let scan_deleted = crate::config::with_config(|bundle| bundle.config.scan_deleted_videos);
    if !scan_deleted {
        query = query.filter(video::Column::Deleted.eq(0));
    }

    // 直接检查是否存在bangumi参数，单独处理
    if let Some(id) = params.bangumi {
        query = query.filter(video::Column::SourceId.eq(id).and(video::Column::SourceType.eq(1)));
    } else {
        // 处理其他常规类型
        for (field, column) in [
            (params.collection, video::Column::CollectionId),
            (params.favorite, video::Column::FavoriteId),
            (params.submission, video::Column::SubmissionId),
            (params.watch_later, video::Column::WatchLaterId),
        ] {
            if let Some(id) = field {
                query = query.filter(column.eq(id));
            }
        }
    }
    if let Some(query_word) = params.query {
        query = query.filter(
            video::Column::Name
                .contains(&query_word)
                .or(video::Column::Path.contains(&query_word)),
        );
    }

    // 筛选失败任务（仅显示下载状态中包含失败的视频）
    if params.show_failed_only.unwrap_or(false) {
        // download_status是u32类型，使用位运算编码5个子任务状态
        // 每3位表示一个子任务：(download_status >> (offset * 3)) & 7
        // 状态值：0=未开始，1-6=失败次数，7=成功
        // 筛选任一子任务状态在1-6范围内的视频
        use sea_orm::sea_query::Expr;

        let mut conditions = Vec::new();

        // 检查5个子任务位置的状态
        for offset in 0..5 {
            let shift = offset * 3;
            // 提取第offset个子任务状态: (download_status >> shift) & 7
            // 检查是否为失败状态: >= 1 AND <= 6
            conditions.push(Expr::cust(format!(
                "((download_status >> {}) & 7) BETWEEN 1 AND 6",
                shift
            )));
        }

        // 使用OR连接：任一子任务失败即匹配
        let mut final_condition = conditions[0].clone();
        for condition in conditions.into_iter().skip(1) {
            final_condition = final_condition.or(condition);
        }

        query = query.filter(final_condition);
    }

    let total_count = query.clone().count(db.as_ref()).await?;
    let (page, page_size) = if let (Some(page), Some(page_size)) = (params.page, params.page_size) {
        (page, page_size)
    } else {
        (1, 10)
    };
    Ok(ApiResponse::ok(VideosResponse {
        videos: query
            .order_by_desc(video::Column::Id)
            .select_only()
            .columns([
                video::Column::Id,
                video::Column::Name,
                video::Column::UpperName,
                video::Column::Path,
                video::Column::Category,
                video::Column::DownloadStatus,
                video::Column::Cover,
            ])
            .into_tuple::<(i32, String, String, String, i32, u32, String)>()
            .paginate(db.as_ref(), page_size)
            .fetch_page(page)
            .await?
            .into_iter()
            .map(VideoInfo::from)
            .collect(),
        total_count,
    }))
}

/// 获取视频详细信息，包括关联的所有 page
#[utoipa::path(
    get,
    path = "/api/videos/{id}",
    responses(
        (status = 200, body = ApiResponse<VideoResponse>),
    )
)]
pub async fn get_video(
    Path(id): Path<i32>,
    Extension(db): Extension<Arc<DatabaseConnection>>,
) -> Result<ApiResponse<VideoResponse>, ApiError> {
    let video_info = video::Entity::find_by_id(id)
        .select_only()
        .columns([
            video::Column::Id,
            video::Column::Name,
            video::Column::UpperName,
            video::Column::Path,
            video::Column::Category,
            video::Column::DownloadStatus,
            video::Column::Cover,
        ])
        .into_tuple::<(i32, String, String, String, i32, u32, String)>()
        .one(db.as_ref())
        .await?
        .map(VideoInfo::from);
    let Some(video_info) = video_info else {
        return Err(InnerApiError::NotFound(id).into());
    };
    let pages = page::Entity::find()
        .filter(page::Column::VideoId.eq(id))
        .order_by_asc(page::Column::Pid)
        .select_only()
        .columns([
            page::Column::Id,
            page::Column::Pid,
            page::Column::Name,
            page::Column::DownloadStatus,
            page::Column::Path,
        ])
        .into_tuple::<(i32, i32, String, u32, Option<String>)>()
        .all(db.as_ref())
        .await?
        .into_iter()
        .map(PageInfo::from)
        .collect();
    Ok(ApiResponse::ok(VideoResponse {
        video: video_info,
        pages,
    }))
}

/// 重置视频的下载状态
#[utoipa::path(
    post,
    path = "/api/videos/{id}/reset",
    params(
        ("id" = i32, Path, description = "Video ID"),
        ("force" = Option<bool>, Query, description = "Force reset all tasks including successful ones")
    ),
    responses(
        (status = 200, body = ApiResponse<ResetVideoResponse>),
    )
)]
pub async fn reset_video(
    Path(id): Path<i32>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    Extension(db): Extension<Arc<DatabaseConnection>>,
) -> Result<ApiResponse<ResetVideoResponse>, ApiError> {
    // 检查是否强制重置
    let force_reset = params
        .get("force")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    // 获取视频和分页信息
    let (video_info, pages_info) = tokio::try_join!(
        video::Entity::find_by_id(id)
            .select_only()
            .columns([
                video::Column::Id,
                video::Column::Name,
                video::Column::UpperName,
                video::Column::Path,
                video::Column::Category,
                video::Column::DownloadStatus,
                video::Column::Cover,
            ])
            .into_tuple::<(i32, String, String, String, i32, u32, String)>()
            .one(db.as_ref()),
        page::Entity::find()
            .filter(page::Column::VideoId.eq(id))
            .order_by_asc(page::Column::Pid)
            .select_only()
            .columns([
                page::Column::Id,
                page::Column::Pid,
                page::Column::Name,
                page::Column::DownloadStatus,
            ])
            .into_tuple::<(i32, i32, String, u32)>()
            .all(db.as_ref())
    )?;

    let Some(video_info) = video_info else {
        return Err(InnerApiError::NotFound(id).into());
    };

    let mut video_info = VideoInfo::from(video_info);
    let resetted_pages_info = pages_info
        .into_iter()
        .filter_map(|(page_id, pid, name, download_status)| {
            let mut page_status = PageStatus::from(download_status);
            let should_reset = if force_reset {
                page_status.reset_all()
            } else {
                page_status.reset_failed()
            };
            if should_reset {
                Some(PageInfo::from((page_id, pid, name, page_status.into())))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let mut video_status = VideoStatus::from(video_info.download_status);
    let mut video_resetted = if force_reset {
        video_status.reset_all()
    } else {
        video_status.reset_failed()
    };

    if !resetted_pages_info.is_empty() {
        video_status.set(4, 0); // 将"分P下载"重置为 0
        video_resetted = true;
    }

    if video_resetted {
        video_info.download_status = video_status.into();
    }

    let resetted = video_resetted || !resetted_pages_info.is_empty();

    if resetted {
        let txn = db.begin().await?;

        if video_resetted {
            video::Entity::update(video::ActiveModel {
                id: Unchanged(id),
                download_status: Set(VideoStatus::from(video_info.download_status).into()),
                ..Default::default()
            })
            .exec(&txn)
            .await?;
        }

        if !resetted_pages_info.is_empty() {
            for page in &resetted_pages_info {
                page::Entity::update(page::ActiveModel {
                    id: Unchanged(page.id),
                    download_status: Set(PageStatus::from(page.download_status).into()),
                    ..Default::default()
                })
                .exec(&txn)
                .await?;
            }
        }

        txn.commit().await?;
    }

    // 获取所有分页信息（包括未重置的）
    let all_pages_info = page::Entity::find()
        .filter(page::Column::VideoId.eq(id))
        .order_by_asc(page::Column::Pid)
        .select_only()
        .columns([
            page::Column::Id,
            page::Column::Pid,
            page::Column::Name,
            page::Column::DownloadStatus,
        ])
        .into_tuple::<(i32, i32, String, u32)>()
        .all(db.as_ref())
        .await?
        .into_iter()
        .map(PageInfo::from)
        .collect();

    Ok(ApiResponse::ok(ResetVideoResponse {
        resetted,
        video: video_info,
        pages: all_pages_info,
    }))
}

/// 重置所有视频和页面的失败状态为未下载状态，这样在下次下载任务中会触发重试
#[utoipa::path(
    post,
    path = "/api/videos/reset-all",
    params(
        ("collection" = Option<i32>, Query, description = "合集ID"),
        ("favorite" = Option<i32>, Query, description = "收藏夹ID"),
        ("submission" = Option<i32>, Query, description = "UP主投稿ID"),
        ("bangumi" = Option<i32>, Query, description = "番剧ID"),
        ("watch_later" = Option<i32>, Query, description = "稍后观看ID"),
    ),
    responses(
        (status = 200, body = ApiResponse<ResetAllVideosResponse>),
    )
)]
pub async fn reset_all_videos(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    Query(params): Query<crate::api::request::VideosRequest>,
) -> Result<ApiResponse<ResetAllVideosResponse>, ApiError> {
    use std::collections::HashSet;

    // 构建查询条件，与get_videos保持一致
    let mut video_query = video::Entity::find();

    // 根据配置决定是否过滤已删除的视频
    let scan_deleted = crate::config::with_config(|bundle| bundle.config.scan_deleted_videos);
    if !scan_deleted {
        video_query = video_query.filter(video::Column::Deleted.eq(0));
    }

    // 直接检查是否存在bangumi参数，单独处理
    if let Some(id) = params.bangumi {
        video_query = video_query.filter(video::Column::SourceId.eq(id).and(video::Column::SourceType.eq(1)));
    } else {
        // 处理其他常规类型
        for (field, column) in [
            (params.collection, video::Column::CollectionId),
            (params.favorite, video::Column::FavoriteId),
            (params.submission, video::Column::SubmissionId),
            (params.watch_later, video::Column::WatchLaterId),
        ] {
            if let Some(id) = field {
                video_query = video_query.filter(column.eq(id));
            }
        }
    }

    // 先查询符合条件的视频和相关页面数据
    let (all_videos, all_pages) = tokio::try_join!(
        video_query
            .select_only()
            .columns([
                video::Column::Id,
                video::Column::Name,
                video::Column::UpperName,
                video::Column::Path,
                video::Column::Category,
                video::Column::DownloadStatus,
                video::Column::Cover,
            ])
            .into_tuple::<(i32, String, String, String, i32, u32, String)>()
            .all(db.as_ref()),
        page::Entity::find()
            .inner_join(video::Entity)
            .filter({
                let mut page_query_filter = Condition::all();

                // 根据配置决定是否过滤已删除的视频
                if !scan_deleted {
                    page_query_filter = page_query_filter.add(video::Column::Deleted.eq(0));
                }

                // 直接检查是否存在bangumi参数，单独处理
                if let Some(id) = params.bangumi {
                    page_query_filter =
                        page_query_filter.add(video::Column::SourceId.eq(id).and(video::Column::SourceType.eq(1)));
                } else {
                    // 处理其他常规类型
                    for (field, column) in [
                        (params.collection, video::Column::CollectionId),
                        (params.favorite, video::Column::FavoriteId),
                        (params.submission, video::Column::SubmissionId),
                        (params.watch_later, video::Column::WatchLaterId),
                    ] {
                        if let Some(id) = field {
                            page_query_filter = page_query_filter.add(column.eq(id));
                        }
                    }
                }

                page_query_filter
            })
            .select_only()
            .columns([
                page::Column::Id,
                page::Column::Pid,
                page::Column::Name,
                page::Column::DownloadStatus,
                page::Column::VideoId,
            ])
            .into_tuple::<(i32, i32, String, u32, i32)>()
            .all(db.as_ref())
    )?;

    // 处理页面重置
    let resetted_pages_info = all_pages
        .into_iter()
        .filter_map(|(id, pid, name, download_status, video_id)| {
            let mut page_status = PageStatus::from(download_status);
            if page_status.reset_failed() {
                let page_info = PageInfo::from((id, pid, name, page_status.into()));
                Some((page_info, video_id))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let video_ids_with_resetted_pages: HashSet<i32> =
        resetted_pages_info.iter().map(|(_, video_id)| *video_id).collect();

    let resetted_pages_info: Vec<PageInfo> = resetted_pages_info
        .into_iter()
        .map(|(page_info, _)| page_info)
        .collect();

    let all_videos_info: Vec<VideoInfo> = all_videos.into_iter().map(VideoInfo::from).collect();

    let resetted_videos_info = all_videos_info
        .into_iter()
        .filter_map(|mut video_info| {
            let mut video_status = VideoStatus::from(video_info.download_status);
            let mut video_resetted = video_status.reset_failed();
            if video_ids_with_resetted_pages.contains(&video_info.id) {
                video_status.set(4, 0); // 将"分P下载"重置为 0
                video_resetted = true;
            }
            if video_resetted {
                video_info.download_status = video_status.into();
                Some(video_info)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let resetted = !(resetted_videos_info.is_empty() && resetted_pages_info.is_empty());

    if resetted {
        let txn = db.begin().await?;

        // 批量更新视频状态
        if !resetted_videos_info.is_empty() {
            for video in &resetted_videos_info {
                video::Entity::update(video::ActiveModel {
                    id: sea_orm::ActiveValue::Unchanged(video.id),
                    download_status: sea_orm::Set(VideoStatus::from(video.download_status).into()),
                    ..Default::default()
                })
                .exec(&txn)
                .await?;
            }
        }

        // 批量更新页面状态
        if !resetted_pages_info.is_empty() {
            for page in &resetted_pages_info {
                page::Entity::update(page::ActiveModel {
                    id: sea_orm::ActiveValue::Unchanged(page.id),
                    download_status: sea_orm::Set(PageStatus::from(page.download_status).into()),
                    ..Default::default()
                })
                .exec(&txn)
                .await?;
            }
        }

        txn.commit().await?;
    }

    Ok(ApiResponse::ok(ResetAllVideosResponse {
        resetted,
        resetted_videos_count: resetted_videos_info.len(),
        resetted_pages_count: resetted_pages_info.len(),
    }))
}

/// 强制重置特定任务状态（不管当前状态）
#[utoipa::path(
    post,
    path = "/api/videos/reset-specific-tasks",
    request_body = ResetSpecificTasksRequest,
    responses(
        (status = 200, body = ApiResponse<ResetAllVideosResponse>),
    )
)]
pub async fn reset_specific_tasks(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    axum::Json(request): axum::Json<crate::api::request::ResetSpecificTasksRequest>,
) -> Result<ApiResponse<ResetAllVideosResponse>, ApiError> {
    use std::collections::HashSet;

    let task_indexes = &request.task_indexes;
    if task_indexes.is_empty() {
        return Err(crate::api::error::InnerApiError::BadRequest("至少需要选择一个任务".to_string()).into());
    }

    // 验证任务索引范围
    for &index in task_indexes {
        if index > 4 {
            return Err(crate::api::error::InnerApiError::BadRequest(format!("无效的任务索引: {}", index)).into());
        }
    }

    // 构建查询条件，与get_videos保持一致
    let mut video_query = video::Entity::find();

    // 根据配置决定是否过滤已删除的视频
    let scan_deleted = crate::config::with_config(|bundle| bundle.config.scan_deleted_videos);
    if !scan_deleted {
        video_query = video_query.filter(video::Column::Deleted.eq(0));
    }

    // 直接检查是否存在bangumi参数，单独处理
    if let Some(id) = request.bangumi {
        video_query = video_query.filter(video::Column::SourceId.eq(id).and(video::Column::SourceType.eq(1)));
    } else {
        // 处理其他常规类型
        for (field, column) in [
            (request.collection, video::Column::CollectionId),
            (request.favorite, video::Column::FavoriteId),
            (request.submission, video::Column::SubmissionId),
            (request.watch_later, video::Column::WatchLaterId),
        ] {
            if let Some(id) = field {
                video_query = video_query.filter(column.eq(id));
            }
        }
    }

    // 先查询符合条件的视频和相关页面数据
    let (all_videos, all_pages) = tokio::try_join!(
        video_query
            .select_only()
            .columns([
                video::Column::Id,
                video::Column::Name,
                video::Column::UpperName,
                video::Column::Path,
                video::Column::Category,
                video::Column::DownloadStatus,
                video::Column::Cover,
            ])
            .into_tuple::<(i32, String, String, String, i32, u32, String)>()
            .all(db.as_ref()),
        page::Entity::find()
            .inner_join(video::Entity)
            .filter({
                let mut page_query_filter = Condition::all();

                // 根据配置决定是否过滤已删除的视频
                if !scan_deleted {
                    page_query_filter = page_query_filter.add(video::Column::Deleted.eq(0));
                }

                // 直接检查是否存在bangumi参数，单独处理
                if let Some(id) = request.bangumi {
                    page_query_filter =
                        page_query_filter.add(video::Column::SourceId.eq(id).and(video::Column::SourceType.eq(1)));
                } else {
                    // 处理其他常规类型
                    for (field, column) in [
                        (request.collection, video::Column::CollectionId),
                        (request.favorite, video::Column::FavoriteId),
                        (request.submission, video::Column::SubmissionId),
                        (request.watch_later, video::Column::WatchLaterId),
                    ] {
                        if let Some(id) = field {
                            page_query_filter = page_query_filter.add(column.eq(id));
                        }
                    }
                }

                page_query_filter
            })
            .select_only()
            .columns([
                page::Column::Id,
                page::Column::Pid,
                page::Column::Name,
                page::Column::DownloadStatus,
                page::Column::VideoId,
            ])
            .into_tuple::<(i32, i32, String, u32, i32)>()
            .all(db.as_ref())
    )?;

    // 处理页面重置 - 强制重置指定任务（不管当前状态）
    let resetted_pages_info = all_pages
        .into_iter()
        .filter_map(|(id, pid, name, download_status, video_id)| {
            let mut page_status = PageStatus::from(download_status);
            let mut page_resetted = false;

            // 强制重置指定的任务索引（不管当前状态）
            for &task_index in task_indexes {
                if task_index < 5 {
                    let current_status = page_status.get(task_index);
                    if current_status != 0 {
                        // 只要不是未开始状态就重置
                        page_status.set(task_index, 0); // 重置为未开始
                        page_resetted = true;
                    }
                }
            }

            if page_resetted {
                let page_info = PageInfo::from((id, pid, name, page_status.into()));
                Some((page_info, video_id))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let video_ids_with_resetted_pages: HashSet<i32> =
        resetted_pages_info.iter().map(|(_, video_id)| *video_id).collect();

    let resetted_pages_info: Vec<PageInfo> = resetted_pages_info
        .into_iter()
        .map(|(page_info, _)| page_info)
        .collect();

    let all_videos_info: Vec<VideoInfo> = all_videos.into_iter().map(VideoInfo::from).collect();

    let resetted_videos_info = all_videos_info
        .into_iter()
        .filter_map(|mut video_info| {
            let mut video_status = VideoStatus::from(video_info.download_status);
            let mut video_resetted = false;

            // 强制重置指定任务（不管当前状态）
            for &task_index in task_indexes {
                if task_index < 5 {
                    let current_status = video_status.get(task_index);
                    if current_status != 0 {
                        // 只要不是未开始状态就重置
                        video_status.set(task_index, 0); // 重置为未开始
                        video_resetted = true;
                    }
                }
            }

            // 如果有分页被重置，同时重置分P下载状态
            if video_ids_with_resetted_pages.contains(&video_info.id) {
                video_status.set(4, 0); // 将"分P下载"重置为 0
                video_resetted = true;
            }

            if video_resetted {
                video_info.download_status = video_status.into();
                Some(video_info)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let resetted = !(resetted_videos_info.is_empty() && resetted_pages_info.is_empty());

    if resetted {
        let txn = db.begin().await?;

        // 批量更新视频状态
        if !resetted_videos_info.is_empty() {
            for video in &resetted_videos_info {
                video::Entity::update(video::ActiveModel {
                    id: sea_orm::ActiveValue::Unchanged(video.id),
                    download_status: sea_orm::Set(VideoStatus::from(video.download_status).into()),
                    ..Default::default()
                })
                .exec(&txn)
                .await?;
            }
        }

        // 批量更新页面状态
        if !resetted_pages_info.is_empty() {
            for page in &resetted_pages_info {
                page::Entity::update(page::ActiveModel {
                    id: sea_orm::ActiveValue::Unchanged(page.id),
                    download_status: sea_orm::Set(PageStatus::from(page.download_status).into()),
                    ..Default::default()
                })
                .exec(&txn)
                .await?;
            }
        }

        txn.commit().await?;
    }

    Ok(ApiResponse::ok(ResetAllVideosResponse {
        resetted,
        resetted_videos_count: resetted_videos_info.len(),
        resetted_pages_count: resetted_pages_info.len(),
    }))
}

/// 更新特定视频及其所含分页的状态位
#[utoipa::path(
    post,
    path = "/api/videos/{id}/update-status",
    request_body = UpdateVideoStatusRequest,
    responses(
        (status = 200, body = ApiResponse<UpdateVideoStatusResponse>),
    )
)]
pub async fn update_video_status(
    Path(id): Path<i32>,
    Extension(db): Extension<Arc<DatabaseConnection>>,
    axum::Json(request): axum::Json<UpdateVideoStatusRequest>,
) -> Result<ApiResponse<UpdateVideoStatusResponse>, ApiError> {
    let (video_info, pages_info) = tokio::try_join!(
        video::Entity::find_by_id(id)
            .select_only()
            .columns([
                video::Column::Id,
                video::Column::Name,
                video::Column::UpperName,
                video::Column::Path,
                video::Column::Category,
                video::Column::DownloadStatus,
                video::Column::Cover,
            ])
            .into_tuple::<(i32, String, String, String, i32, u32, String)>()
            .one(db.as_ref()),
        page::Entity::find()
            .filter(page::Column::VideoId.eq(id))
            .order_by_asc(page::Column::Cid)
            .select_only()
            .columns([
                page::Column::Id,
                page::Column::Pid,
                page::Column::Name,
                page::Column::DownloadStatus,
            ])
            .into_tuple::<(i32, i32, String, u32)>()
            .all(db.as_ref())
    )?;

    let Some(video_info) = video_info else {
        return Err(InnerApiError::NotFound(id).into());
    };

    let mut video_info = VideoInfo::from(video_info);
    let mut video_status = VideoStatus::from(video_info.download_status);

    // 应用视频状态更新
    for update in &request.video_updates {
        if update.status_index < 5 {
            video_status.set(update.status_index, update.status_value);
        }
    }
    video_info.download_status = video_status.into();

    let mut pages_info: Vec<PageInfo> = pages_info.into_iter().map(PageInfo::from).collect();

    let mut updated_pages_info = Vec::new();
    let mut page_id_map = pages_info
        .iter_mut()
        .map(|page| (page.id, page))
        .collect::<std::collections::HashMap<_, _>>();

    // 应用页面状态更新
    for page_update in &request.page_updates {
        if let Some(page_info) = page_id_map.remove(&page_update.page_id) {
            let mut page_status = PageStatus::from(page_info.download_status);
            for update in &page_update.updates {
                if update.status_index < 5 {
                    page_status.set(update.status_index, update.status_value);
                }
            }
            page_info.download_status = page_status.into();
            updated_pages_info.push(page_info);
        }
    }

    let has_video_updates = !request.video_updates.is_empty();
    let has_page_updates = !updated_pages_info.is_empty();

    if has_video_updates || has_page_updates {
        let txn = db.begin().await?;

        if has_video_updates {
            video::Entity::update(video::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(video_info.id),
                download_status: sea_orm::Set(VideoStatus::from(video_info.download_status).into()),
                ..Default::default()
            })
            .exec(&txn)
            .await?;
        }

        if has_page_updates {
            for page in &updated_pages_info {
                page::Entity::update(page::ActiveModel {
                    id: sea_orm::ActiveValue::Unchanged(page.id),
                    download_status: sea_orm::Set(PageStatus::from(page.download_status).into()),
                    ..Default::default()
                })
                .exec(&txn)
                .await?;
            }
        }

        txn.commit().await?;
    }

    Ok(ApiResponse::ok(UpdateVideoStatusResponse {
        success: has_video_updates || has_page_updates,
        video: video_info,
        pages: pages_info,
    }))
}

/// 添加新的视频源
#[utoipa::path(
    post,
    path = "/api/video-sources",
    request_body = AddVideoSourceRequest,
    responses(
        (status = 200, body = ApiResponse<AddVideoSourceResponse>),
    )
)]
pub async fn add_video_source(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    axum::Json(params): axum::Json<AddVideoSourceRequest>,
) -> Result<ApiResponse<AddVideoSourceResponse>, ApiError> {
    // 检查是否正在扫描
    if crate::task::is_scanning() {
        // 正在扫描，将添加任务加入队列
        let task_id = uuid::Uuid::new_v4().to_string();
        let add_task = crate::task::AddVideoSourceTask {
            source_type: params.source_type.clone(),
            name: params.name.clone(),
            source_id: params.source_id.clone(),
            path: params.path.clone(),
            up_id: params.up_id.clone(),
            collection_type: params.collection_type.clone(),
            media_id: params.media_id.clone(),
            ep_id: params.ep_id.clone(),
            download_all_seasons: params.download_all_seasons,
            selected_seasons: params.selected_seasons.clone(),
            task_id: task_id.clone(),
        };

        crate::task::enqueue_add_task(add_task, &db).await?;

        info!(
            "检测到正在扫描，添加任务已加入队列等待处理: {} 名称={}",
            params.source_type, params.name
        );

        return Ok(ApiResponse::ok(AddVideoSourceResponse {
            success: true,
            source_id: 0, // 队列中的任务还没有ID
            source_type: params.source_type,
            message: "正在扫描中，添加任务已加入队列，将在扫描完成后自动处理".to_string(),
        }));
    }

    // 没有扫描，直接执行添加
    match add_video_source_internal(db, params).await {
        Ok(response) => Ok(ApiResponse::ok(response)),
        Err(e) => Err(e),
    }
}

/// 内部添加视频源函数（用于队列处理和直接调用）
pub async fn add_video_source_internal(
    db: Arc<DatabaseConnection>,
    params: AddVideoSourceRequest,
) -> Result<AddVideoSourceResponse, ApiError> {
    let txn = db.begin().await?;

    let result = match params.source_type.as_str() {
        "collection" => {
            // 验证合集必需的参数
            let up_id_str = params
                .up_id
                .as_ref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow!("合集类型需要提供UP主ID"))?;

            let up_id = up_id_str.parse::<i64>().map_err(|_| anyhow!("无效的UP主ID"))?;
            let s_id = params.source_id.parse::<i64>().map_err(|_| anyhow!("无效的合集ID"))?;

            // 检查是否已存在相同的合集
            let existing_collection = collection::Entity::find()
                .filter(collection::Column::SId.eq(s_id))
                .filter(collection::Column::MId.eq(up_id))
                .one(&txn)
                .await?;

            if let Some(existing) = existing_collection {
                return Err(anyhow!(
                    "合集已存在！合集名称：\"{}\"，合集ID：{}，UP主ID：{}，保存路径：{}。如需修改设置，请先删除现有合集再重新添加。",
                    existing.name,
                    existing.s_id,
                    existing.m_id,
                    existing.path
                ).into());
            }

            // 添加合集
            let collection_type_value = params.collection_type.as_deref().unwrap_or("season");
            let collection_type = match collection_type_value {
                "season" => 2, // 视频合集
                "series" => 1, // 视频列表
                _ => 2,        // 默认使用season类型
            };

            let collection = collection::ActiveModel {
                id: sea_orm::ActiveValue::NotSet,
                s_id: sea_orm::Set(s_id),
                m_id: sea_orm::Set(up_id),
                name: sea_orm::Set(params.name),
                r#type: sea_orm::Set(collection_type),
                path: sea_orm::Set(params.path.clone()),
                created_at: sea_orm::Set(chrono::Local::now().to_string()),
                latest_row_at: sea_orm::Set(chrono::NaiveDateTime::default()),
                enabled: sea_orm::Set(true),
                scan_deleted_videos: sea_orm::Set(false),
            };

            let insert_result = collection::Entity::insert(collection).exec(&txn).await?;

            AddVideoSourceResponse {
                success: true,
                source_id: insert_result.last_insert_id,
                source_type: "collection".to_string(),
                message: "合集添加成功".to_string(),
            }
        }
        "favorite" => {
            let f_id = params.source_id.parse::<i64>().map_err(|_| anyhow!("无效的收藏夹ID"))?;

            // 检查是否已存在相同的收藏夹
            let existing_favorite = favorite::Entity::find()
                .filter(favorite::Column::FId.eq(f_id))
                .one(&txn)
                .await?;

            if let Some(existing) = existing_favorite {
                return Err(anyhow!(
                    "收藏夹已存在！收藏夹名称：\"{}\"，收藏夹ID：{}，保存路径：{}。如需修改设置，请先删除现有收藏夹再重新添加。",
                    existing.name,
                    existing.f_id,
                    existing.path
                ).into());
            }

            // 添加收藏夹
            let favorite = favorite::ActiveModel {
                id: sea_orm::ActiveValue::NotSet,
                f_id: sea_orm::Set(f_id),
                name: sea_orm::Set(params.name),
                path: sea_orm::Set(params.path.clone()),
                created_at: sea_orm::Set(chrono::Local::now().to_string()),
                latest_row_at: sea_orm::Set(chrono::NaiveDateTime::default()),
                enabled: sea_orm::Set(true),
                scan_deleted_videos: sea_orm::Set(false),
            };

            let insert_result = favorite::Entity::insert(favorite).exec(&txn).await?;

            AddVideoSourceResponse {
                success: true,
                source_id: insert_result.last_insert_id,
                source_type: "favorite".to_string(),
                message: "收藏夹添加成功".to_string(),
            }
        }
        "submission" => {
            let upper_id = params.source_id.parse::<i64>().map_err(|_| anyhow!("无效的UP主ID"))?;

            // 检查是否已存在相同的UP主投稿
            let existing_submission = submission::Entity::find()
                .filter(submission::Column::UpperId.eq(upper_id))
                .one(&txn)
                .await?;

            if let Some(existing) = existing_submission {
                return Err(anyhow!(
                    "UP主投稿已存在！UP主名称：\"{}\"，UP主ID：{}，保存路径：{}。如需修改设置，请先删除现有UP主投稿再重新添加。",
                    existing.upper_name,
                    existing.upper_id,
                    existing.path
                ).into());
            }

            // 添加UP主投稿
            let submission = submission::ActiveModel {
                id: sea_orm::ActiveValue::NotSet,
                upper_id: sea_orm::Set(upper_id),
                upper_name: sea_orm::Set(params.name),
                path: sea_orm::Set(params.path.clone()),
                created_at: sea_orm::Set(chrono::Local::now().to_string()),
                latest_row_at: sea_orm::Set(chrono::NaiveDateTime::default()),
                enabled: sea_orm::Set(true),
                scan_deleted_videos: sea_orm::Set(false),
            };

            let insert_result = submission::Entity::insert(submission).exec(&txn).await?;

            AddVideoSourceResponse {
                success: true,
                source_id: insert_result.last_insert_id,
                source_type: "submission".to_string(),
                message: "UP主投稿添加成功".to_string(),
            }
        }
        "bangumi" => {
            // 验证至少有一个ID不为空
            if params.source_id.is_empty() && params.media_id.is_none() && params.ep_id.is_none() {
                return Err(anyhow!("番剧标识不能全部为空，请至少提供 season_id、media_id 或 ep_id 中的一个").into());
            }

            // 检查是否已存在相同的番剧（Season ID完全匹配）
            let existing_query = video_source::Entity::find().filter(video_source::Column::Type.eq(1)); // 番剧类型

            // 1. 首先检查 Season ID 是否重复（精确匹配）
            let mut existing_bangumi = None;

            if !params.source_id.is_empty() {
                // 如果有 season_id，检查是否已存在该 season_id
                existing_bangumi = existing_query
                    .clone()
                    .filter(video_source::Column::SeasonId.eq(&params.source_id))
                    .one(&txn)
                    .await?;
            }

            if existing_bangumi.is_none() {
                if let Some(ref media_id) = params.media_id {
                    // 如果只有 media_id，检查是否已存在该 media_id
                    existing_bangumi = existing_query
                        .clone()
                        .filter(video_source::Column::MediaId.eq(media_id))
                        .one(&txn)
                        .await?;
                } else if let Some(ref ep_id) = params.ep_id {
                    // 如果只有 ep_id，检查是否已存在该 ep_id
                    existing_bangumi = existing_query
                        .clone()
                        .filter(video_source::Column::EpId.eq(ep_id))
                        .one(&txn)
                        .await?;
                }
            }

            if let Some(mut existing) = existing_bangumi {
                // 情况1：Season ID 重复 → 合并到现有番剧源
                info!("检测到重复番剧 Season ID，执行智能合并: {}", existing.name);

                let download_all_seasons = params.download_all_seasons.unwrap_or(false);
                let mut updated = false;
                let mut merge_message = String::new();

                // 如果新请求要下载全部季度，直接更新现有配置
                if download_all_seasons {
                    if !existing.download_all_seasons.unwrap_or(false) {
                        existing.download_all_seasons = Some(true);
                        existing.selected_seasons = None; // 清空特定季度选择
                        updated = true;
                        merge_message = "已更新为下载全部季度".to_string();
                    } else {
                        merge_message = "已配置为下载全部季度，无需更改".to_string();
                    }
                } else {
                    // 处理特定季度的合并
                    if let Some(new_seasons) = params.selected_seasons {
                        if !new_seasons.is_empty() {
                            let mut current_seasons: Vec<String> = Vec::new();

                            // 获取现有的季度选择
                            if let Some(ref seasons_json) = existing.selected_seasons {
                                if let Ok(seasons) = serde_json::from_str::<Vec<String>>(seasons_json) {
                                    current_seasons = seasons;
                                }
                            }

                            // 合并新的季度（去重）
                            let mut all_seasons = current_seasons.clone();
                            let mut added_seasons = Vec::new();

                            for season in new_seasons {
                                if !all_seasons.contains(&season) {
                                    all_seasons.push(season.clone());
                                    added_seasons.push(season);
                                }
                            }

                            if !added_seasons.is_empty() {
                                // 有新季度需要添加
                                let seasons_json = serde_json::to_string(&all_seasons)?;
                                existing.selected_seasons = Some(seasons_json);
                                existing.download_all_seasons = Some(false); // 确保不是全部下载模式
                                updated = true;

                                merge_message = if added_seasons.len() == 1 {
                                    format!("已添加新季度: {}", added_seasons.join(", "))
                                } else {
                                    format!("已添加 {} 个新季度: {}", added_seasons.len(), added_seasons.join(", "))
                                };
                            } else {
                                // 所有季度都已存在
                                merge_message = "所选季度已存在于现有配置中，无需更改".to_string();
                            }
                        }
                    }
                }

                // 更新保存路径（如果提供了不同的路径）
                if !params.path.is_empty() && params.path != existing.path {
                    existing.path = params.path.clone();
                    updated = true;

                    if !merge_message.is_empty() {
                        merge_message.push('，');
                    }
                    merge_message.push_str(&format!("保存路径已更新为: {}", params.path));
                }

                // 更新番剧名称（如果提供了不同的名称）
                if !params.name.is_empty() && params.name != existing.name {
                    existing.name = params.name.clone();
                    updated = true;

                    if !merge_message.is_empty() {
                        merge_message.push('，');
                    }
                    merge_message.push_str(&format!("番剧名称已更新为: {}", params.name));
                }

                if updated {
                    // 更新数据库记录 - 修复：正确使用ActiveModel更新
                    let mut existing_update = video_source::ActiveModel {
                        id: sea_orm::ActiveValue::Unchanged(existing.id),
                        latest_row_at: sea_orm::Set(chrono::Utc::now().naive_utc()),
                        ..Default::default()
                    };

                    // 根据实际修改的字段设置对应的ActiveModel字段
                    if download_all_seasons && !existing.download_all_seasons.unwrap_or(false) {
                        // 切换到下载全部季度模式
                        existing_update.download_all_seasons = sea_orm::Set(Some(true));
                        existing_update.selected_seasons = sea_orm::Set(None); // 清空特定季度选择
                    } else if !download_all_seasons {
                        // 处理特定季度的合并或更新
                        if let Some(ref new_seasons_json) = existing.selected_seasons {
                            existing_update.selected_seasons = sea_orm::Set(Some(new_seasons_json.clone()));
                            existing_update.download_all_seasons = sea_orm::Set(Some(false));
                        }
                    }

                    // 更新路径（如果有变更）
                    if !params.path.is_empty() && params.path != existing.path {
                        existing_update.path = sea_orm::Set(params.path.clone());
                    }

                    // 更新名称（如果有变更）
                    if !params.name.is_empty() && params.name != existing.name {
                        existing_update.name = sea_orm::Set(params.name.clone());
                    }

                    video_source::Entity::update(existing_update).exec(&txn).await?;

                    // 确保目标路径存在
                    std::fs::create_dir_all(&existing.path).map_err(|e| anyhow!("创建目录失败: {}", e))?;

                    info!("番剧配置合并成功: {}", merge_message);

                    AddVideoSourceResponse {
                        success: true,
                        source_id: existing.id,
                        source_type: "bangumi".to_string(),
                        message: format!("番剧配置已成功合并！{}", merge_message),
                    }
                } else {
                    // 没有实际更新
                    AddVideoSourceResponse {
                        success: true,
                        source_id: existing.id,
                        source_type: "bangumi".to_string(),
                        message: format!("番剧已存在，{}", merge_message),
                    }
                }
            } else {
                // 情况2：Season ID 不重复，检查季度重复并跳过
                let download_all_seasons = params.download_all_seasons.unwrap_or(false);
                let mut final_selected_seasons = params.selected_seasons.clone();
                let mut skipped_seasons = Vec::new();

                // 如果不是下载全部季度，且指定了特定季度，则检查季度重复
                if !download_all_seasons {
                    if let Some(ref new_seasons) = params.selected_seasons {
                        if !new_seasons.is_empty() {
                            // 获取所有现有番剧源的已选季度
                            let all_existing_sources = video_source::Entity::find()
                                .filter(video_source::Column::Type.eq(1))
                                .all(&txn)
                                .await?;

                            let mut all_existing_seasons = std::collections::HashSet::new();

                            for source in all_existing_sources {
                                // 如果该番剧源配置为下载全部季度，我们无法确定具体季度，跳过检查
                                if source.download_all_seasons.unwrap_or(false) {
                                    continue;
                                }

                                // 获取该番剧源的已选季度
                                if let Some(ref seasons_json) = source.selected_seasons {
                                    if let Ok(seasons) = serde_json::from_str::<Vec<String>>(seasons_json) {
                                        for season in seasons {
                                            all_existing_seasons.insert(season);
                                        }
                                    }
                                }
                            }

                            // 过滤掉重复的季度
                            let mut unique_seasons = Vec::new();
                            for season in new_seasons {
                                if all_existing_seasons.contains(season) {
                                    skipped_seasons.push(season.clone());
                                } else {
                                    unique_seasons.push(season.clone());
                                }
                            }

                            final_selected_seasons = Some(unique_seasons);
                        }
                    }
                }

                // 如果所有季度都被跳过了，返回错误
                if !download_all_seasons && final_selected_seasons.as_ref().is_none_or(|s| s.is_empty()) {
                    let skipped_msg = if skipped_seasons.is_empty() {
                        "未选择任何季度".to_string()
                    } else {
                        format!("所选季度已在其他番剧源中存在，已跳过: {}", skipped_seasons.join(", "))
                    };

                    return Err(anyhow!(
                        "无法添加番剧：{}。请选择其他季度或使用'下载全部季度'选项。",
                        skipped_msg
                    )
                    .into());
                }

                // 处理选中的季度
                let selected_seasons_json = if !download_all_seasons && final_selected_seasons.is_some() {
                    let seasons = final_selected_seasons.clone().unwrap();
                    if seasons.is_empty() {
                        None
                    } else {
                        Some(serde_json::to_string(&seasons)?)
                    }
                } else {
                    None
                };

                let bangumi = video_source::ActiveModel {
                    id: sea_orm::ActiveValue::NotSet,
                    name: sea_orm::Set(params.name),
                    path: sea_orm::Set(params.path.clone()),
                    r#type: sea_orm::Set(1), // 1表示番剧类型
                    latest_row_at: sea_orm::Set(chrono::Utc::now().naive_utc()),
                    season_id: sea_orm::Set(Some(params.source_id.clone())),
                    media_id: sea_orm::Set(params.media_id),
                    ep_id: sea_orm::Set(params.ep_id),
                    download_all_seasons: sea_orm::Set(Some(download_all_seasons)),
                    selected_seasons: sea_orm::Set(selected_seasons_json),
                    ..Default::default()
                };

                let insert_result = video_source::Entity::insert(bangumi).exec(&txn).await?;

                // 确保目标路径存在
                std::fs::create_dir_all(&params.path).map_err(|e| anyhow!("创建目录失败: {}", e))?;

                let success_message = if !skipped_seasons.is_empty() {
                    format!(
                        "番剧添加成功！已跳过重复季度: {}，添加的季度: {}",
                        skipped_seasons.join(", "),
                        final_selected_seasons.unwrap_or_default().join(", ")
                    )
                } else {
                    "番剧添加成功".to_string()
                };

                info!("新番剧添加完成: {}", success_message);

                AddVideoSourceResponse {
                    success: true,
                    source_id: insert_result.last_insert_id,
                    source_type: "bangumi".to_string(),
                    message: success_message,
                }
            }
        }
        "watch_later" => {
            // 稍后观看只能有一个，检查是否已存在
            let existing = watch_later::Entity::find().count(&txn).await?;

            if existing > 0 {
                // 获取现有的稍后观看配置信息
                let existing_watch_later = watch_later::Entity::find()
                    .one(&txn)
                    .await?
                    .ok_or_else(|| anyhow!("数据库状态异常"))?;

                return Err(anyhow!(
                    "稍后观看已存在！保存路径：{}。一个系统只能配置一个稍后观看源，如需修改路径，请先删除现有配置再重新添加。",
                    existing_watch_later.path
                ).into());
            }

            let watch_later = watch_later::ActiveModel {
                id: sea_orm::ActiveValue::NotSet,
                path: sea_orm::Set(params.path.clone()),
                latest_row_at: sea_orm::Set(chrono::Utc::now().naive_utc()),
                enabled: sea_orm::Set(true),
                ..Default::default()
            };

            let insert_result = watch_later::Entity::insert(watch_later).exec(&txn).await?;

            AddVideoSourceResponse {
                success: true,
                source_id: insert_result.last_insert_id,
                source_type: "watch_later".to_string(),
                message: "稍后观看添加成功".to_string(),
            }
        }
        _ => return Err(anyhow!("不支持的视频源类型: {}", params.source_type).into()),
    };

    // 确保目标路径存在
    std::fs::create_dir_all(&params.path).map_err(|e| anyhow!("创建目录失败: {}", e))?;

    txn.commit().await?;
    Ok(result)
}

/// 重新加载配置
#[utoipa::path(
    post,
    path = "/api/reload-config",
    responses(
        (status = 200, body = ApiResponse<bool>),
    )
)]
pub async fn reload_config(Extension(db): Extension<Arc<DatabaseConnection>>) -> Result<ApiResponse<bool>, ApiError> {
    // 检查是否正在扫描
    if crate::task::is_scanning() {
        // 正在扫描，将重载配置任务加入队列
        let task_id = uuid::Uuid::new_v4().to_string();
        let reload_task = crate::task::ReloadConfigTask {
            task_id: task_id.clone(),
        };

        crate::task::enqueue_reload_task(reload_task, &db).await?;

        info!("检测到正在扫描，重载配置任务已加入队列等待处理");

        return Ok(ApiResponse::ok(true));
    }

    // 没有扫描，直接执行重载配置
    match reload_config_internal().await {
        Ok(result) => Ok(ApiResponse::ok(result)),
        Err(e) => Err(e),
    }
}

/// 内部重载配置函数（用于队列处理和直接调用）
pub async fn reload_config_internal() -> Result<bool, ApiError> {
    // 优先从数据库重新加载配置包
    if let Err(e) = crate::config::reload_config_bundle().await {
        warn!("从数据库重新加载配置包失败: {}, 回退到TOML重载", e);
        // 回退到传统的重新加载方式
        let _new_config = crate::config::reload_config();
    } else {
        info!("配置包已从数据库重新加载");
    }

    info!("配置已重新加载");

    // 返回成功响应
    Ok(true)
}

/// 更新视频源启用状态
#[utoipa::path(
    put,
    path = "/api/video-sources/{source_type}/{id}/enabled",
    params(
        ("source_type" = String, Path, description = "视频源类型"),
        ("id" = i32, Path, description = "视频源ID"),
    ),
    request_body = crate::api::request::UpdateVideoSourceEnabledRequest,
    responses(
        (status = 200, body = ApiResponse<crate::api::response::UpdateVideoSourceEnabledResponse>),
    )
)]
pub async fn update_video_source_enabled(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    Path((source_type, id)): Path<(String, i32)>,
    axum::Json(params): axum::Json<crate::api::request::UpdateVideoSourceEnabledRequest>,
) -> Result<ApiResponse<crate::api::response::UpdateVideoSourceEnabledResponse>, ApiError> {
    update_video_source_enabled_internal(db, source_type, id, params.enabled)
        .await
        .map(ApiResponse::ok)
}

/// 内部更新视频源启用状态函数
pub async fn update_video_source_enabled_internal(
    db: Arc<DatabaseConnection>,
    source_type: String,
    id: i32,
    enabled: bool,
) -> Result<crate::api::response::UpdateVideoSourceEnabledResponse, ApiError> {
    let txn = db.begin().await?;

    let result = match source_type.as_str() {
        "collection" => {
            let collection = collection::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的合集"))?;

            collection::Entity::update(collection::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                enabled: sea_orm::Set(enabled),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceEnabledResponse {
                success: true,
                source_id: id,
                source_type: "collection".to_string(),
                enabled,
                message: format!("合集 {} 已{}", collection.name, if enabled { "启用" } else { "禁用" }),
            }
        }
        "favorite" => {
            let favorite = favorite::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的收藏夹"))?;

            favorite::Entity::update(favorite::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                enabled: sea_orm::Set(enabled),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceEnabledResponse {
                success: true,
                source_id: id,
                source_type: "favorite".to_string(),
                enabled,
                message: format!("收藏夹 {} 已{}", favorite.name, if enabled { "启用" } else { "禁用" }),
            }
        }
        "submission" => {
            let submission = submission::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的UP主投稿"))?;

            submission::Entity::update(submission::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                enabled: sea_orm::Set(enabled),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceEnabledResponse {
                success: true,
                source_id: id,
                source_type: "submission".to_string(),
                enabled,
                message: format!(
                    "UP主投稿 {} 已{}",
                    submission.upper_name,
                    if enabled { "启用" } else { "禁用" }
                ),
            }
        }
        "watch_later" => {
            let _watch_later = watch_later::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的稍后观看"))?;

            watch_later::Entity::update(watch_later::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                enabled: sea_orm::Set(enabled),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceEnabledResponse {
                success: true,
                source_id: id,
                source_type: "watch_later".to_string(),
                enabled,
                message: format!("稍后观看已{}", if enabled { "启用" } else { "禁用" }),
            }
        }
        "bangumi" => {
            let bangumi = video_source::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的番剧"))?;

            video_source::Entity::update(video_source::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                enabled: sea_orm::Set(enabled),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceEnabledResponse {
                success: true,
                source_id: id,
                source_type: "bangumi".to_string(),
                enabled,
                message: format!("番剧 {} 已{}", bangumi.name, if enabled { "启用" } else { "禁用" }),
            }
        }
        _ => {
            return Err(anyhow!("不支持的视频源类型: {}", source_type).into());
        }
    };

    txn.commit().await?;
    Ok(result)
}

/// 删除视频源
#[utoipa::path(
    delete,
    path = "/api/video-sources/{source_type}/{id}",
    params(
        ("source_type" = String, Path, description = "视频源类型"),
        ("id" = i32, Path, description = "视频源ID"),
        ("delete_local_files" = bool, Query, description = "是否删除本地文件")
    ),
    responses(
        (status = 200, body = ApiResponse<DeleteVideoSourceResponse>),
    )
)]
pub async fn delete_video_source(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    Path((source_type, id)): Path<(String, i32)>,
    Query(params): Query<crate::api::request::DeleteVideoSourceRequest>,
) -> Result<ApiResponse<crate::api::response::DeleteVideoSourceResponse>, ApiError> {
    let delete_local_files = params.delete_local_files;

    // 检查是否正在扫描
    if crate::task::is_scanning() {
        // 正在扫描，将删除任务加入队列
        let task_id = uuid::Uuid::new_v4().to_string();
        let delete_task = crate::task::DeleteVideoSourceTask {
            source_type: source_type.clone(),
            source_id: id,
            delete_local_files,
            task_id: task_id.clone(),
        };

        crate::task::enqueue_delete_task(delete_task, &db).await?;

        info!("检测到正在扫描，删除任务已加入队列等待处理: {} ID={}", source_type, id);

        return Ok(ApiResponse::ok(crate::api::response::DeleteVideoSourceResponse {
            success: true,
            source_id: id,
            source_type,
            message: "正在扫描中，删除任务已加入队列，将在扫描完成后自动处理".to_string(),
        }));
    }

    // 没有扫描，直接执行删除
    match delete_video_source_internal(db, source_type, id, delete_local_files).await {
        Ok(response) => Ok(ApiResponse::ok(response)),
        Err(e) => Err(e),
    }
}

/// 删除单个视频（软删除）
#[utoipa::path(
    delete,
    path = "/api/videos/{id}",
    params(
        ("id" = i32, description = "视频ID")
    ),
    responses(
        (status = 200, body = ApiResponse<DeleteVideoResponse>),
    )
)]
pub async fn delete_video(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<crate::api::response::DeleteVideoResponse>, ApiError> {
    // 检查是否正在扫描
    if crate::task::is_scanning() {
        // 正在扫描，将删除任务加入队列
        let task_id = uuid::Uuid::new_v4().to_string();
        let delete_task = crate::task::DeleteVideoTask {
            video_id: id,
            task_id: task_id.clone(),
        };

        crate::task::enqueue_video_delete_task(delete_task, &db).await?;

        info!("检测到正在扫描，视频删除任务已加入队列等待处理: 视频ID={}", id);

        return Ok(ApiResponse::ok(crate::api::response::DeleteVideoResponse {
            success: true,
            video_id: id,
            message: "正在扫描中，视频删除任务已加入队列，将在扫描完成后自动处理".to_string(),
        }));
    }

    // 没有扫描，直接执行删除
    match delete_video_internal(db, id).await {
        Ok(_) => Ok(ApiResponse::ok(crate::api::response::DeleteVideoResponse {
            success: true,
            video_id: id,
            message: "视频已成功删除".to_string(),
        })),
        Err(e) => Err(e),
    }
}

/// 内部删除视频函数（用于队列处理和直接调用）
pub async fn delete_video_internal(db: Arc<DatabaseConnection>, video_id: i32) -> Result<(), ApiError> {
    use bili_sync_entity::video;
    use sea_orm::*;

    // 检查视频是否存在
    let video = video::Entity::find_by_id(video_id).one(db.as_ref()).await?;

    let video = match video {
        Some(v) => v,
        None => {
            return Err(crate::api::error::InnerApiError::NotFound(video_id).into());
        }
    };

    // 检查是否已经删除
    if video.deleted == 1 {
        return Err(crate::api::error::InnerApiError::BadRequest("视频已经被删除".to_string()).into());
    }

    // 删除本地文件 - 根据page表中的路径精确删除
    let deleted_files = delete_video_files_from_pages(db.clone(), video_id).await?;

    if deleted_files > 0 {
        info!("已删除 {} 个视频文件", deleted_files);

        // 检查视频文件夹是否为空，如果为空则删除文件夹
        let normalized_video_path = normalize_file_path(&video.path);
        let video_path = std::path::Path::new(&normalized_video_path);
        if video_path.exists() {
            match tokio::fs::read_dir(&normalized_video_path).await {
                Ok(mut entries) => {
                    if entries.next_entry().await.unwrap_or(None).is_none() {
                        // 文件夹为空，删除它
                        if let Err(e) = std::fs::remove_dir(&normalized_video_path) {
                            warn!("删除空文件夹失败: {} - {}", normalized_video_path, e);
                        } else {
                            info!("已删除空文件夹: {}", normalized_video_path);
                        }
                    }
                }
                Err(e) => {
                    warn!("读取文件夹失败: {} - {}", normalized_video_path, e);
                }
            }
        }
    } else {
        debug!("未找到需要删除的文件，视频ID: {}", video_id);
    }

    // 执行软删除：将deleted字段设为1
    video::Entity::update_many()
        .col_expr(video::Column::Deleted, sea_orm::prelude::Expr::value(1))
        .filter(video::Column::Id.eq(video_id))
        .exec(db.as_ref())
        .await?;

    info!("视频已成功删除: ID={}, 名称={}", video_id, video.name);

    Ok(())
}

/// 根据page表精确删除视频文件
async fn delete_video_files_from_pages(db: Arc<DatabaseConnection>, video_id: i32) -> Result<usize, ApiError> {
    use tokio::fs;

    // 获取该视频的所有页面（分P）
    let pages = page::Entity::find()
        .filter(page::Column::VideoId.eq(video_id))
        .all(db.as_ref())
        .await?;

    let mut deleted_count = 0;

    for page in pages {
        if let Some(file_path) = &page.path {
            let path = std::path::Path::new(file_path);
            info!("尝试删除视频文件: {}", file_path);
            if path.exists() {
                match fs::remove_file(path).await {
                    Ok(_) => {
                        debug!("已删除视频文件: {}", file_path);
                        deleted_count += 1;
                    }
                    Err(e) => {
                        warn!("删除视频文件失败: {} - {}", file_path, e);
                    }
                }
            } else {
                debug!("文件不存在，跳过删除: {}", file_path);
            }
        }

        // 同时删除封面图片（如果存在且是本地文件）
        if let Some(image_path) = &page.image {
            // 跳过HTTP URL，只处理本地文件路径
            if !image_path.starts_with("http://") && !image_path.starts_with("https://") {
                let path = std::path::Path::new(image_path);
                info!("尝试删除封面图片: {}", image_path);
                if path.exists() {
                    match fs::remove_file(path).await {
                        Ok(_) => {
                            info!("已删除封面图片: {}", image_path);
                            deleted_count += 1;
                        }
                        Err(e) => {
                            warn!("删除封面图片失败: {} - {}", image_path, e);
                        }
                    }
                } else {
                    debug!("封面图片文件不存在，跳过删除: {}", image_path);
                }
            } else {
                debug!("跳过远程封面图片URL: {}", image_path);
            }
        }
    }

    // 还要删除视频的NFO文件和其他可能的相关文件
    let video = video::Entity::find_by_id(video_id).one(db.as_ref()).await?;

    if let Some(_video) = video {
        // 获取页面信息来删除基于视频文件名的相关文件
        let pages = page::Entity::find()
            .filter(page::Column::VideoId.eq(video_id))
            .all(db.as_ref())
            .await?;

        for page in pages {
            if let Some(file_path) = &page.path {
                let video_file = std::path::Path::new(file_path);
                if let Some(parent_dir) = video_file.parent() {
                    if let Some(file_stem) = video_file.file_stem() {
                        let file_stem_str = file_stem.to_string_lossy();

                        // 删除同名的NFO文件
                        let nfo_path = parent_dir.join(format!("{}.nfo", file_stem_str));
                        if nfo_path.exists() {
                            match fs::remove_file(&nfo_path).await {
                                Ok(_) => {
                                    debug!("已删除NFO文件: {:?}", nfo_path);
                                    deleted_count += 1;
                                }
                                Err(e) => {
                                    warn!("删除NFO文件失败: {:?} - {}", nfo_path, e);
                                }
                            }
                        }

                        // 删除封面文件 (-fanart.jpg, -poster.jpg等)
                        for suffix in &["fanart", "poster"] {
                            for ext in &["jpg", "jpeg", "png", "webp"] {
                                let cover_path = parent_dir.join(format!("{}-{}.{}", file_stem_str, suffix, ext));
                                if cover_path.exists() {
                                    match fs::remove_file(&cover_path).await {
                                        Ok(_) => {
                                            debug!("已删除封面文件: {:?}", cover_path);
                                            deleted_count += 1;
                                        }
                                        Err(e) => {
                                            warn!("删除封面文件失败: {:?} - {}", cover_path, e);
                                        }
                                    }
                                }
                            }
                        }

                        // 删除弹幕文件 (.zh-CN.default.ass等)
                        let danmaku_patterns = [
                            format!("{}.zh-CN.default.ass", file_stem_str),
                            format!("{}.ass", file_stem_str),
                            format!("{}.srt", file_stem_str),
                            format!("{}.xml", file_stem_str),
                        ];

                        for pattern in &danmaku_patterns {
                            let danmaku_path = parent_dir.join(pattern);
                            if danmaku_path.exists() {
                                match fs::remove_file(&danmaku_path).await {
                                    Ok(_) => {
                                        debug!("已删除弹幕文件: {:?}", danmaku_path);
                                        deleted_count += 1;
                                    }
                                    Err(e) => {
                                        warn!("删除弹幕文件失败: {:?} - {}", danmaku_path, e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(deleted_count)
}

/// 内部删除视频源函数（用于队列处理和直接调用）
pub async fn delete_video_source_internal(
    db: Arc<DatabaseConnection>,
    source_type: String,
    id: i32,
    delete_local_files: bool,
) -> Result<crate::api::response::DeleteVideoSourceResponse, ApiError> {
    let txn = db.begin().await?;

    // 根据不同类型的视频源执行不同的删除操作
    let result = match source_type.as_str() {
        "collection" => {
            // 查找要删除的合集
            let collection = collection::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的合集"))?;

            // 删除相关的视频和页面数据
            let videos = video::Entity::find()
                .filter(video::Column::CollectionId.eq(id))
                .all(&txn)
                .await?;

            for video in &videos {
                // 删除页面数据
                page::Entity::delete_many()
                    .filter(page::Column::VideoId.eq(video.id))
                    .exec(&txn)
                    .await?;
            }

            // 删除视频数据
            video::Entity::delete_many()
                .filter(video::Column::CollectionId.eq(id))
                .exec(&txn)
                .await?;

            // 如果需要删除本地文件
            if delete_local_files {
                // 添加安全检查
                let base_path = &collection.path;
                if base_path.is_empty() || base_path == "/" || base_path == "\\" {
                    warn!("检测到危险路径，跳过删除: {}", base_path);
                } else {
                    // 删除合集相关的具体视频文件夹，而不是删除整个合集基础目录
                    info!("开始删除合集 {} 的相关文件夹", collection.name);

                    // 获取所有相关的视频记录来确定需要删除的具体文件夹
                    let mut deleted_folders = std::collections::HashSet::new();
                    let mut total_deleted_size = 0u64;

                    for video in &videos {
                        // 对于每个视频，删除其对应的文件夹
                        let video_path = std::path::Path::new(&video.path);

                        if video_path.exists() && !deleted_folders.contains(&video.path) {
                            match get_directory_size(&video.path) {
                                Ok(size) => {
                                    let size_mb = size as f64 / 1024.0 / 1024.0;
                                    info!("删除合集视频文件夹: {} (大小: {:.2} MB)", video.path, size_mb);

                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除合集视频文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除合集视频文件夹: {} ({:.2} MB)", video.path, size_mb);
                                        deleted_folders.insert(video.path.clone());
                                        total_deleted_size += size;
                                    }
                                }
                                Err(e) => {
                                    warn!("无法计算文件夹大小: {} - {}", video.path, e);
                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除合集视频文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除合集视频文件夹: {}", video.path);
                                        deleted_folders.insert(video.path.clone());
                                    }
                                }
                            }
                        }
                    }

                    if !deleted_folders.is_empty() {
                        let total_size_mb = total_deleted_size as f64 / 1024.0 / 1024.0;
                        info!(
                            "合集 {} 删除完成，共删除 {} 个文件夹，总大小: {:.2} MB",
                            collection.name,
                            deleted_folders.len(),
                            total_size_mb
                        );
                    } else {
                        info!("合集 {} 没有找到需要删除的本地文件夹", collection.name);
                    }
                }
            }

            // 删除数据库中的记录
            collection::Entity::delete_by_id(id).exec(&txn).await?;

            crate::api::response::DeleteVideoSourceResponse {
                success: true,
                source_id: id,
                source_type: "collection".to_string(),
                message: format!("合集 {} 已成功删除", collection.name),
            }
        }
        "favorite" => {
            // 查找要删除的收藏夹
            let favorite = favorite::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的收藏夹"))?;

            // 删除相关的视频和页面数据
            let videos = video::Entity::find()
                .filter(video::Column::FavoriteId.eq(id))
                .all(&txn)
                .await?;

            for video in &videos {
                // 删除页面数据
                page::Entity::delete_many()
                    .filter(page::Column::VideoId.eq(video.id))
                    .exec(&txn)
                    .await?;
            }

            // 删除视频数据
            video::Entity::delete_many()
                .filter(video::Column::FavoriteId.eq(id))
                .exec(&txn)
                .await?;

            // 如果需要删除本地文件
            if delete_local_files {
                let base_path = &favorite.path;
                if base_path.is_empty() || base_path == "/" || base_path == "\\" {
                    warn!("检测到危险路径，跳过删除: {}", base_path);
                } else {
                    // 删除收藏夹相关的具体视频文件夹，而不是删除整个收藏夹基础目录
                    info!("开始删除收藏夹 {} 的相关文件夹", favorite.name);

                    // 获取所有相关的视频记录来确定需要删除的具体文件夹
                    let mut deleted_folders = std::collections::HashSet::new();
                    let mut total_deleted_size = 0u64;

                    for video in &videos {
                        // 对于每个视频，删除其对应的文件夹
                        let video_path = std::path::Path::new(&video.path);

                        if video_path.exists() && !deleted_folders.contains(&video.path) {
                            match get_directory_size(&video.path) {
                                Ok(size) => {
                                    let size_mb = size as f64 / 1024.0 / 1024.0;
                                    info!("删除收藏夹视频文件夹: {} (大小: {:.2} MB)", video.path, size_mb);

                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除收藏夹视频文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除收藏夹视频文件夹: {} ({:.2} MB)", video.path, size_mb);
                                        deleted_folders.insert(video.path.clone());
                                        total_deleted_size += size;
                                    }
                                }
                                Err(e) => {
                                    warn!("无法计算文件夹大小: {} - {}", video.path, e);
                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除收藏夹视频文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除收藏夹视频文件夹: {}", video.path);
                                        deleted_folders.insert(video.path.clone());
                                    }
                                }
                            }
                        }
                    }

                    if !deleted_folders.is_empty() {
                        let total_size_mb = total_deleted_size as f64 / 1024.0 / 1024.0;
                        info!(
                            "收藏夹 {} 删除完成，共删除 {} 个文件夹，总大小: {:.2} MB",
                            favorite.name,
                            deleted_folders.len(),
                            total_size_mb
                        );
                    } else {
                        info!("收藏夹 {} 没有找到需要删除的本地文件夹", favorite.name);
                    }
                }
            }

            // 删除数据库中的记录
            favorite::Entity::delete_by_id(id).exec(&txn).await?;

            crate::api::response::DeleteVideoSourceResponse {
                success: true,
                source_id: id,
                source_type: "favorite".to_string(),
                message: format!("收藏夹 {} 已成功删除", favorite.name),
            }
        }
        "submission" => {
            // 查找要删除的UP主投稿
            let submission = submission::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的UP主投稿"))?;

            // 删除相关的视频和页面数据
            let videos = video::Entity::find()
                .filter(video::Column::SubmissionId.eq(id))
                .all(&txn)
                .await?;

            for video in &videos {
                // 删除页面数据
                page::Entity::delete_many()
                    .filter(page::Column::VideoId.eq(video.id))
                    .exec(&txn)
                    .await?;
            }

            // 删除视频数据
            video::Entity::delete_many()
                .filter(video::Column::SubmissionId.eq(id))
                .exec(&txn)
                .await?;

            // 如果需要删除本地文件
            if delete_local_files {
                let base_path = &submission.path;
                if base_path.is_empty() || base_path == "/" || base_path == "\\" {
                    warn!("检测到危险路径，跳过删除: {}", base_path);
                } else {
                    // 删除UP主投稿相关的具体视频文件夹，而不是删除整个UP主投稿基础目录
                    info!("开始删除UP主投稿 {} 的相关文件夹", submission.upper_name);

                    // 获取所有相关的视频记录来确定需要删除的具体文件夹
                    let mut deleted_folders = std::collections::HashSet::new();
                    let mut total_deleted_size = 0u64;

                    for video in &videos {
                        // 对于每个视频，删除其对应的文件夹
                        let video_path = std::path::Path::new(&video.path);

                        if video_path.exists() && !deleted_folders.contains(&video.path) {
                            match get_directory_size(&video.path) {
                                Ok(size) => {
                                    let size_mb = size as f64 / 1024.0 / 1024.0;
                                    info!("删除UP主投稿视频文件夹: {} (大小: {:.2} MB)", video.path, size_mb);

                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除UP主投稿视频文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除UP主投稿视频文件夹: {} ({:.2} MB)", video.path, size_mb);
                                        deleted_folders.insert(video.path.clone());
                                        total_deleted_size += size;
                                    }
                                }
                                Err(e) => {
                                    warn!("无法计算文件夹大小: {} - {}", video.path, e);
                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除UP主投稿视频文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除UP主投稿视频文件夹: {}", video.path);
                                        deleted_folders.insert(video.path.clone());
                                    }
                                }
                            }
                        }
                    }

                    if !deleted_folders.is_empty() {
                        let total_size_mb = total_deleted_size as f64 / 1024.0 / 1024.0;
                        info!(
                            "UP主投稿 {} 删除完成，共删除 {} 个文件夹，总大小: {:.2} MB",
                            submission.upper_name,
                            deleted_folders.len(),
                            total_size_mb
                        );
                    } else {
                        info!("UP主投稿 {} 没有找到需要删除的本地文件夹", submission.upper_name);
                    }
                }
            }

            // 删除数据库中的记录
            submission::Entity::delete_by_id(id).exec(&txn).await?;

            crate::api::response::DeleteVideoSourceResponse {
                success: true,
                source_id: id,
                source_type: "submission".to_string(),
                message: format!("UP主 {} 的投稿已成功删除", submission.upper_name),
            }
        }
        "watch_later" => {
            // 查找要删除的稍后再看
            let watch_later = watch_later::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的稍后再看"))?;

            // 删除相关的视频和页面数据
            let videos = video::Entity::find()
                .filter(video::Column::WatchLaterId.eq(id))
                .all(&txn)
                .await?;

            for video in &videos {
                // 删除页面数据
                page::Entity::delete_many()
                    .filter(page::Column::VideoId.eq(video.id))
                    .exec(&txn)
                    .await?;
            }

            // 删除视频数据
            video::Entity::delete_many()
                .filter(video::Column::WatchLaterId.eq(id))
                .exec(&txn)
                .await?;

            // 如果需要删除本地文件
            if delete_local_files {
                let base_path = &watch_later.path;
                if base_path.is_empty() || base_path == "/" || base_path == "\\" {
                    warn!("检测到危险路径，跳过删除: {}", base_path);
                } else {
                    // 删除稍后再看相关的具体视频文件夹，而不是删除整个稍后再看基础目录
                    info!("开始删除稍后再看的相关文件夹");

                    // 获取所有相关的视频记录来确定需要删除的具体文件夹
                    let mut deleted_folders = std::collections::HashSet::new();
                    let mut total_deleted_size = 0u64;

                    for video in &videos {
                        // 对于每个视频，删除其对应的文件夹
                        let video_path = std::path::Path::new(&video.path);

                        if video_path.exists() && !deleted_folders.contains(&video.path) {
                            match get_directory_size(&video.path) {
                                Ok(size) => {
                                    let size_mb = size as f64 / 1024.0 / 1024.0;
                                    info!("删除稍后再看视频文件夹: {} (大小: {:.2} MB)", video.path, size_mb);

                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除稍后再看视频文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除稍后再看视频文件夹: {} ({:.2} MB)", video.path, size_mb);
                                        deleted_folders.insert(video.path.clone());
                                        total_deleted_size += size;
                                    }
                                }
                                Err(e) => {
                                    warn!("无法计算文件夹大小: {} - {}", video.path, e);
                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除稍后再看视频文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除稍后再看视频文件夹: {}", video.path);
                                        deleted_folders.insert(video.path.clone());
                                    }
                                }
                            }
                        }
                    }

                    if !deleted_folders.is_empty() {
                        let total_size_mb = total_deleted_size as f64 / 1024.0 / 1024.0;
                        info!(
                            "稍后再看删除完成，共删除 {} 个文件夹，总大小: {:.2} MB",
                            deleted_folders.len(),
                            total_size_mb
                        );
                    } else {
                        info!("稍后再看没有找到需要删除的本地文件夹");
                    }
                }
            }

            // 删除数据库中的记录
            watch_later::Entity::delete_by_id(id).exec(&txn).await?;

            crate::api::response::DeleteVideoSourceResponse {
                success: true,
                source_id: id,
                source_type: "watch_later".to_string(),
                message: "稍后再看已成功删除".to_string(),
            }
        }
        "bangumi" => {
            // 查找要删除的番剧
            let bangumi = video_source::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的番剧"))?;

            // 删除相关的视频和页面数据
            let videos = video::Entity::find()
                .filter(video::Column::SourceId.eq(id))
                .filter(video::Column::SourceType.eq(1)) // 番剧类型
                .all(&txn)
                .await?;

            for video in &videos {
                // 删除页面数据
                page::Entity::delete_many()
                    .filter(page::Column::VideoId.eq(video.id))
                    .exec(&txn)
                    .await?;
            }

            // 删除视频数据
            video::Entity::delete_many()
                .filter(video::Column::SourceId.eq(id))
                .filter(video::Column::SourceType.eq(1))
                .exec(&txn)
                .await?;

            // 如果需要删除本地文件
            if delete_local_files {
                let base_path = &bangumi.path;
                if base_path.is_empty() || base_path == "/" || base_path == "\\" {
                    warn!("检测到危险路径，跳过删除: {}", base_path);
                } else {
                    // 删除番剧相关的季度文件夹，而不是删除整个番剧基础目录
                    info!("开始删除番剧 {} 的相关文件夹", bangumi.name);

                    // 获取所有相关的视频记录来确定需要删除的具体文件夹
                    let mut deleted_folders = std::collections::HashSet::new();
                    let mut total_deleted_size = 0u64;

                    for video in &videos {
                        // 对于每个视频，删除其对应的文件夹
                        let video_path = std::path::Path::new(&video.path);

                        if video_path.exists() && !deleted_folders.contains(&video.path) {
                            match get_directory_size(&video.path) {
                                Ok(size) => {
                                    let size_mb = size as f64 / 1024.0 / 1024.0;
                                    info!("删除番剧季度文件夹: {} (大小: {:.2} MB)", video.path, size_mb);

                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除番剧季度文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除番剧季度文件夹: {} ({:.2} MB)", video.path, size_mb);
                                        deleted_folders.insert(video.path.clone());
                                        total_deleted_size += size;
                                    }
                                }
                                Err(e) => {
                                    warn!("无法计算文件夹大小: {} - {}", video.path, e);
                                    if let Err(e) = std::fs::remove_dir_all(&video.path) {
                                        error!("删除番剧季度文件夹失败: {} - {}", video.path, e);
                                    } else {
                                        info!("成功删除番剧季度文件夹: {}", video.path);
                                        deleted_folders.insert(video.path.clone());
                                    }
                                }
                            }
                        }
                    }

                    if !deleted_folders.is_empty() {
                        let total_size_mb = total_deleted_size as f64 / 1024.0 / 1024.0;
                        info!(
                            "番剧 {} 删除完成，共删除 {} 个文件夹，总大小: {:.2} MB",
                            bangumi.name,
                            deleted_folders.len(),
                            total_size_mb
                        );
                    } else {
                        info!("番剧 {} 没有找到需要删除的本地文件夹", bangumi.name);
                    }
                }
            }

            // 删除数据库中的记录
            video_source::Entity::delete_by_id(id).exec(&txn).await?;

            crate::api::response::DeleteVideoSourceResponse {
                success: true,
                source_id: id,
                source_type: "bangumi".to_string(),
                message: format!("番剧 {} 已成功删除", bangumi.name),
            }
        }
        _ => return Err(anyhow!("不支持的视频源类型: {}", source_type).into()),
    };

    txn.commit().await?;
    Ok(result)
}

/// 更新视频源扫描已删除视频设置
#[utoipa::path(
    put,
    path = "/api/video-sources/{source_type}/{id}/scan-deleted",
    params(
        ("source_type" = String, Path, description = "视频源类型"),
        ("id" = i32, Path, description = "视频源ID"),
    ),
    request_body = crate::api::request::UpdateVideoSourceScanDeletedRequest,
    responses(
        (status = 200, body = ApiResponse<crate::api::response::UpdateVideoSourceScanDeletedResponse>),
    )
)]
pub async fn update_video_source_scan_deleted(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    Path((source_type, id)): Path<(String, i32)>,
    axum::Json(params): axum::Json<crate::api::request::UpdateVideoSourceScanDeletedRequest>,
) -> Result<ApiResponse<crate::api::response::UpdateVideoSourceScanDeletedResponse>, ApiError> {
    update_video_source_scan_deleted_internal(db, source_type, id, params.scan_deleted_videos)
        .await
        .map(ApiResponse::ok)
}

/// 内部更新视频源扫描已删除视频设置函数
pub async fn update_video_source_scan_deleted_internal(
    db: Arc<DatabaseConnection>,
    source_type: String,
    id: i32,
    scan_deleted_videos: bool,
) -> Result<crate::api::response::UpdateVideoSourceScanDeletedResponse, ApiError> {
    let txn = db.begin().await?;

    let result = match source_type.as_str() {
        "collection" => {
            let collection = collection::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的合集"))?;

            collection::Entity::update(collection::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                scan_deleted_videos: sea_orm::Set(scan_deleted_videos),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceScanDeletedResponse {
                success: true,
                source_id: id,
                source_type: "collection".to_string(),
                scan_deleted_videos,
                message: format!(
                    "合集 {} 的扫描已删除视频设置已{}",
                    collection.name,
                    if scan_deleted_videos { "启用" } else { "禁用" }
                ),
            }
        }
        "favorite" => {
            let favorite = favorite::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的收藏夹"))?;

            favorite::Entity::update(favorite::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                scan_deleted_videos: sea_orm::Set(scan_deleted_videos),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceScanDeletedResponse {
                success: true,
                source_id: id,
                source_type: "favorite".to_string(),
                scan_deleted_videos,
                message: format!(
                    "收藏夹 {} 的扫描已删除视频设置已{}",
                    favorite.name,
                    if scan_deleted_videos { "启用" } else { "禁用" }
                ),
            }
        }
        "submission" => {
            let submission = submission::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的UP主投稿"))?;

            submission::Entity::update(submission::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                scan_deleted_videos: sea_orm::Set(scan_deleted_videos),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceScanDeletedResponse {
                success: true,
                source_id: id,
                source_type: "submission".to_string(),
                scan_deleted_videos,
                message: format!(
                    "UP主投稿 {} 的扫描已删除视频设置已{}",
                    submission.upper_name,
                    if scan_deleted_videos { "启用" } else { "禁用" }
                ),
            }
        }
        "watch_later" => {
            let _watch_later = watch_later::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的稍后观看"))?;

            watch_later::Entity::update(watch_later::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                scan_deleted_videos: sea_orm::Set(scan_deleted_videos),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceScanDeletedResponse {
                success: true,
                source_id: id,
                source_type: "watch_later".to_string(),
                scan_deleted_videos,
                message: format!(
                    "稍后观看的扫描已删除视频设置已{}",
                    if scan_deleted_videos { "启用" } else { "禁用" }
                ),
            }
        }
        "bangumi" => {
            let video_source = video_source::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的番剧"))?;

            video_source::Entity::update(video_source::ActiveModel {
                id: sea_orm::ActiveValue::Unchanged(id),
                scan_deleted_videos: sea_orm::Set(scan_deleted_videos),
                ..Default::default()
            })
            .exec(&txn)
            .await?;

            crate::api::response::UpdateVideoSourceScanDeletedResponse {
                success: true,
                source_id: id,
                source_type: "bangumi".to_string(),
                scan_deleted_videos,
                message: format!(
                    "番剧 {} 的扫描已删除视频设置已{}",
                    video_source.name,
                    if scan_deleted_videos { "启用" } else { "禁用" }
                ),
            }
        }
        _ => return Err(anyhow!("不支持的视频源类型: {}", source_type).into()),
    };

    txn.commit().await?;
    Ok(result)
}

/// 删除视频（软删除）
/// 重设视频源路径
#[utoipa::path(
    post,
    path = "/api/video-sources/{source_type}/{id}/reset-path",
    request_body = ResetVideoSourcePathRequest,
    responses(
        (status = 200, body = ApiResponse<ResetVideoSourcePathResponse>),
    )
)]
pub async fn reset_video_source_path(
    Path((source_type, id)): Path<(String, i32)>,
    Extension(db): Extension<Arc<DatabaseConnection>>,
    axum::Json(request): axum::Json<ResetVideoSourcePathRequest>,
) -> Result<ApiResponse<ResetVideoSourcePathResponse>, ApiError> {
    match reset_video_source_path_internal(db, source_type, id, request).await {
        Ok(response) => Ok(ApiResponse::ok(response)),
        Err(e) => Err(e),
    }
}

/// 内部路径重设函数（用于队列处理和直接调用）
pub async fn reset_video_source_path_internal(
    db: Arc<DatabaseConnection>,
    source_type: String,
    id: i32,
    request: ResetVideoSourcePathRequest,
) -> Result<ResetVideoSourcePathResponse, ApiError> {
    let txn = db.begin().await?;
    let mut moved_files_count = 0;
    let mut updated_videos_count = 0;
    let mut cleaned_folders_count = 0;

    // 根据不同类型的视频源执行不同的路径重设操作
    let result = match source_type.as_str() {
        "collection" => {
            let collection = collection::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的合集"))?;
            let old_path = collection.path.clone();

            if request.apply_rename_rules {
                // 获取所有相关视频，按新路径规则移动文件
                let videos = video::Entity::find()
                    .filter(video::Column::CollectionId.eq(id))
                    .all(&txn)
                    .await?;

                for video in &videos {
                    // 移动视频文件到新路径结构
                    match move_video_files_to_new_path(video, &old_path, &request.new_path, request.clean_empty_folders)
                        .await
                    {
                        Ok((moved, cleaned)) => {
                            moved_files_count += moved;
                            cleaned_folders_count += cleaned;
                        }
                        Err(e) => warn!("移动视频 {} 文件失败: {}", video.id, e),
                    }

                    // 重新生成视频和分页的路径
                    if let Err(e) = regenerate_video_and_page_paths_correctly(&txn, video.id, &request.new_path).await {
                        warn!("更新视频 {} 路径失败: {:?}", video.id, e);
                    }
                }
                updated_videos_count = videos.len();
            }

            // 更新数据库中的路径
            collection::Entity::update_many()
                .filter(collection::Column::Id.eq(id))
                .col_expr(collection::Column::Path, Expr::value(request.new_path.clone()))
                .exec(&txn)
                .await?;

            ResetVideoSourcePathResponse {
                success: true,
                source_id: id,
                source_type: "collection".to_string(),
                old_path,
                new_path: request.new_path,
                moved_files_count,
                updated_videos_count,
                cleaned_folders_count,
                message: format!("合集 {} 路径重设完成", collection.name),
            }
        }
        "favorite" => {
            let favorite = favorite::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的收藏夹"))?;
            let old_path = favorite.path.clone();

            if request.apply_rename_rules {
                // 获取所有相关视频，按新路径规则移动文件
                let videos = video::Entity::find()
                    .filter(video::Column::FavoriteId.eq(id))
                    .all(&txn)
                    .await?;

                for video in &videos {
                    // 移动视频文件到新路径结构
                    match move_video_files_to_new_path(video, &old_path, &request.new_path, request.clean_empty_folders)
                        .await
                    {
                        Ok((moved, cleaned)) => {
                            moved_files_count += moved;
                            cleaned_folders_count += cleaned;
                        }
                        Err(e) => warn!("移动视频 {} 文件失败: {}", video.id, e),
                    }

                    // 重新生成视频和分页的路径
                    if let Err(e) = regenerate_video_and_page_paths_correctly(&txn, video.id, &request.new_path).await {
                        warn!("更新视频 {} 路径失败: {:?}", video.id, e);
                    }
                }
                updated_videos_count = videos.len();
            }

            favorite::Entity::update_many()
                .filter(favorite::Column::Id.eq(id))
                .col_expr(favorite::Column::Path, Expr::value(request.new_path.clone()))
                .exec(&txn)
                .await?;

            ResetVideoSourcePathResponse {
                success: true,
                source_id: id,
                source_type: "favorite".to_string(),
                old_path,
                new_path: request.new_path,
                moved_files_count,
                updated_videos_count,
                cleaned_folders_count,
                message: format!("收藏夹 {} 路径重设完成", favorite.name),
            }
        }
        "submission" => {
            let submission = submission::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的UP主投稿"))?;
            let old_path = submission.path.clone();

            if request.apply_rename_rules {
                // 获取所有相关视频，按新路径规则移动文件
                let videos = video::Entity::find()
                    .filter(video::Column::SubmissionId.eq(id))
                    .all(&txn)
                    .await?;

                for video in &videos {
                    // 移动视频文件到新路径结构
                    match move_video_files_to_new_path(video, &old_path, &request.new_path, request.clean_empty_folders)
                        .await
                    {
                        Ok((moved, cleaned)) => {
                            moved_files_count += moved;
                            cleaned_folders_count += cleaned;
                        }
                        Err(e) => warn!("移动视频 {} 文件失败: {}", video.id, e),
                    }

                    // 重新生成视频和分页的路径
                    if let Err(e) = regenerate_video_and_page_paths_correctly(&txn, video.id, &request.new_path).await {
                        warn!("更新视频 {} 路径失败: {:?}", video.id, e);
                    }
                }
                updated_videos_count = videos.len();
            }

            submission::Entity::update_many()
                .filter(submission::Column::Id.eq(id))
                .col_expr(submission::Column::Path, Expr::value(request.new_path.clone()))
                .exec(&txn)
                .await?;

            ResetVideoSourcePathResponse {
                success: true,
                source_id: id,
                source_type: "submission".to_string(),
                old_path,
                new_path: request.new_path,
                moved_files_count,
                updated_videos_count,
                cleaned_folders_count,
                message: format!("UP主投稿 {} 路径重设完成", submission.upper_name),
            }
        }
        "watch_later" => {
            let watch_later = watch_later::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的稍后再看"))?;
            let old_path = watch_later.path.clone();

            if request.apply_rename_rules {
                // 获取所有相关视频，按新路径规则移动文件
                let videos = video::Entity::find()
                    .filter(video::Column::WatchLaterId.eq(id))
                    .all(&txn)
                    .await?;

                for video in &videos {
                    // 移动视频文件到新路径结构
                    match move_video_files_to_new_path(video, &old_path, &request.new_path, request.clean_empty_folders)
                        .await
                    {
                        Ok((moved, cleaned)) => {
                            moved_files_count += moved;
                            cleaned_folders_count += cleaned;
                        }
                        Err(e) => warn!("移动视频 {} 文件失败: {}", video.id, e),
                    }

                    // 重新生成视频和分页的路径
                    if let Err(e) = regenerate_video_and_page_paths_correctly(&txn, video.id, &request.new_path).await {
                        warn!("更新视频 {} 路径失败: {:?}", video.id, e);
                    }
                }
                updated_videos_count = videos.len();
            }

            watch_later::Entity::update_many()
                .filter(watch_later::Column::Id.eq(id))
                .col_expr(watch_later::Column::Path, Expr::value(request.new_path.clone()))
                .exec(&txn)
                .await?;

            ResetVideoSourcePathResponse {
                success: true,
                source_id: id,
                source_type: "watch_later".to_string(),
                old_path,
                new_path: request.new_path,
                moved_files_count,
                updated_videos_count,
                cleaned_folders_count,
                message: "稍后再看路径重设完成".to_string(),
            }
        }
        "bangumi" => {
            let bangumi = video_source::Entity::find_by_id(id)
                .one(&txn)
                .await?
                .ok_or_else(|| anyhow!("未找到指定的番剧"))?;
            let old_path = bangumi.path.clone();

            if request.apply_rename_rules {
                // 获取所有相关视频，按新路径规则移动文件
                let videos = video::Entity::find()
                    .filter(video::Column::SourceId.eq(id))
                    .filter(video::Column::SourceType.eq(1)) // 番剧类型
                    .all(&txn)
                    .await?;

                for video in &videos {
                    // 使用番剧专用的文件移动函数
                    match move_bangumi_files_to_new_path(
                        video,
                        &old_path,
                        &request.new_path,
                        request.clean_empty_folders,
                        &txn,
                    )
                    .await
                    {
                        Ok((moved, cleaned)) => {
                            moved_files_count += moved;
                            cleaned_folders_count += cleaned;
                        }
                        Err(e) => warn!("移动番剧 {} 文件失败: {}", video.id, e),
                    }
                }
                updated_videos_count = videos.len();
            }

            video_source::Entity::update_many()
                .filter(video_source::Column::Id.eq(id))
                .col_expr(video_source::Column::Path, Expr::value(request.new_path.clone()))
                .exec(&txn)
                .await?;

            ResetVideoSourcePathResponse {
                success: true,
                source_id: id,
                source_type: "bangumi".to_string(),
                old_path,
                new_path: request.new_path,
                moved_files_count,
                updated_videos_count,
                cleaned_folders_count,
                message: format!("番剧 {} 路径重设完成", bangumi.name),
            }
        }
        _ => return Err(anyhow!("不支持的视频源类型: {}", source_type).into()),
    };

    txn.commit().await?;
    Ok(result)
}

/// 使用四步重命名原则移动文件夹（直接移动到指定目标路径）
async fn move_files_with_four_step_rename(old_path: &str, target_path: &str) -> Result<String, std::io::Error> {
    use std::path::Path;

    let old_path = Path::new(old_path);
    let target_path = Path::new(target_path);

    if !old_path.exists() {
        return Ok(target_path.to_string_lossy().to_string()); // 如果原路径不存在，返回目标路径
    }

    // 如果目标路径已存在且和源路径相同，无需移动
    if old_path == target_path {
        return Ok(target_path.to_string_lossy().to_string());
    }

    // 确保目标目录的父目录存在
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 四步重命名原则：
    // 1. 重命名到临时名称（在源目录下）
    let temp_name = format!(".temp_{}", chrono::Utc::now().timestamp_millis());
    let temp_path = old_path
        .parent()
        .ok_or_else(|| std::io::Error::other("无法获取父目录"))?
        .join(&temp_name);

    // 2. 移动到目标父目录（使用临时名称）
    let temp_target_path = target_path
        .parent()
        .ok_or_else(|| std::io::Error::other("无法获取目标父目录"))?
        .join(&temp_name);

    // 步骤1: 重命名到临时名称
    std::fs::rename(old_path, &temp_path)?;

    // 步骤2: 移动到目标目录
    std::fs::rename(&temp_path, &temp_target_path)?;

    // 步骤3: 重命名为最终名称
    let final_path = if target_path.exists() {
        // 如果目标已存在，使用冲突解决策略
        let mut counter = 1;
        let target_parent = target_path.parent().unwrap();
        let target_name = target_path.file_name().unwrap();

        loop {
            let conflict_name = format!("{}_{}", target_name.to_string_lossy(), counter);
            let conflict_path = target_parent.join(&conflict_name);
            if !conflict_path.exists() {
                std::fs::rename(&temp_target_path, &conflict_path)?;
                break conflict_path;
            }
            counter += 1;
        }
    } else {
        std::fs::rename(&temp_target_path, target_path)?;
        target_path.to_path_buf()
    };

    Ok(final_path.to_string_lossy().to_string())
}

/// 移动视频文件到新路径结构，返回(移动的文件数量, 清理的文件夹数量)
async fn move_video_files_to_new_path(
    video: &video::Model,
    _old_base_path: &str,
    new_base_path: &str,
    clean_empty_folders: bool,
) -> Result<(usize, usize), std::io::Error> {
    use std::path::Path;

    let mut moved_count = 0;
    let mut cleaned_count = 0;

    // 获取当前视频的存储路径
    let current_video_path = Path::new(&video.path);
    if !current_video_path.exists() {
        return Ok((0, 0)); // 如果视频文件夹不存在，跳过
    }

    // 使用模板重新生成视频在新基础路径下的目标路径
    let new_video_dir = Path::new(new_base_path);

    // 基于视频模型重新生成路径结构
    let new_video_path = crate::config::with_config(|bundle| {
        let video_args = crate::utils::format_arg::video_format_args(video);
        bundle.render_video_template(&video_args)
    })
    .map_err(|e| std::io::Error::other(format!("模板渲染失败: {}", e)))?;

    let target_video_dir = new_video_dir.join(&new_video_path);

    // 如果目标路径和当前路径相同，无需移动
    if current_video_path == target_video_dir {
        return Ok((0, 0));
    }

    // 使用四步重命名原则移动整个视频文件夹
    if (move_files_with_four_step_rename(
        &current_video_path.to_string_lossy(),
        &target_video_dir.to_string_lossy(),
    )
    .await)
        .is_ok()
    {
        moved_count = 1;

        // 移动成功后，检查并清理原来的父目录（如果启用了清理且为空）
        if clean_empty_folders {
            if let Some(parent_dir) = current_video_path.parent() {
                if let Ok(count) = cleanup_empty_directory(parent_dir).await {
                    cleaned_count = count;
                }
            }
        }
    }

    Ok((moved_count, cleaned_count))
}

/// 正确重新生成视频和分页路径（基于新的基础路径重新计算完整路径）
async fn regenerate_video_and_page_paths_correctly(
    txn: &sea_orm::DatabaseTransaction,
    video_id: i32,
    new_base_path: &str,
) -> Result<(), ApiError> {
    use std::path::Path;

    // 获取视频信息
    let video = video::Entity::find_by_id(video_id)
        .one(txn)
        .await?
        .ok_or_else(|| anyhow!("未找到视频记录"))?;

    // 重新生成视频路径
    let new_video_path = crate::config::with_config(|bundle| {
        let video_args = crate::utils::format_arg::video_format_args(&video);
        bundle.render_video_template(&video_args)
    })
    .map_err(|e| anyhow!("视频路径模板渲染失败: {}", e))?;

    let full_new_video_path = Path::new(new_base_path).join(&new_video_path);

    // 更新视频路径
    video::Entity::update_many()
        .filter(video::Column::Id.eq(video_id))
        .col_expr(
            video::Column::Path,
            Expr::value(full_new_video_path.to_string_lossy().to_string()),
        )
        .exec(txn)
        .await?;

    // 更新相关分页路径
    let pages = page::Entity::find()
        .filter(page::Column::VideoId.eq(video_id))
        .all(txn)
        .await?;

    for page_model in pages {
        // 重新生成分页路径
        let new_page_path = crate::config::with_config(|bundle| {
            let page_args = crate::utils::format_arg::page_format_args(&video, &page_model);
            bundle.render_page_template(&page_args)
        })
        .map_err(|e| anyhow!("分页路径模板渲染失败: {}", e))?;

        let full_new_page_path = full_new_video_path.join(format!("{}.mp4", new_page_path));

        page::Entity::update_many()
            .filter(page::Column::Id.eq(page_model.id))
            .col_expr(
                page::Column::Path,
                Expr::value(Some(full_new_page_path.to_string_lossy().to_string())),
            )
            .exec(txn)
            .await?;
    }

    Ok(())
}

/// 递归清理空目录（从指定目录开始向上清理）
async fn cleanup_empty_directory(dir_path: &std::path::Path) -> Result<usize, std::io::Error> {
    use tokio::fs;

    let mut cleaned_count = 0;
    let mut current_dir = dir_path;

    // 从当前目录开始，向上递归检查并清理空目录
    loop {
        if !current_dir.exists() {
            break;
        }

        // 检查目录是否为空
        let mut entries = fs::read_dir(current_dir).await?;
        if entries.next_entry().await?.is_none() {
            // 目录为空，可以删除
            match fs::remove_dir(current_dir).await {
                Ok(_) => {
                    cleaned_count += 1;
                    debug!("清理空目录: {}", current_dir.display());

                    // 继续检查父目录
                    if let Some(parent) = current_dir.parent() {
                        current_dir = parent;
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    debug!("清理目录失败 {}: {}", current_dir.display(), e);
                    break;
                }
            }
        } else {
            // 目录不为空，停止清理
            break;
        }
    }

    Ok(cleaned_count)
}

/// 更新配置文件的辅助函数
// 移除配置文件操作 - 配置现在完全基于数据库
#[allow(dead_code)]
fn update_config_file<F>(update_fn: F) -> Result<()>
where
    F: FnOnce(&mut crate::config::Config) -> Result<()>,
{
    // 重新加载当前配置
    let mut config = crate::config::reload_config();

    // 应用更新函数
    update_fn(&mut config)?;

    // 移除配置文件保存 - 配置现在完全基于数据库
    // config.save()?;

    // 保存配置到数据库
    if let Some(manager) = crate::config::get_config_manager() {
        if let Err(e) =
            tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(manager.save_config(&config)))
        {
            warn!("保存配置到数据库失败: {}", e);
        } else {
            info!("配置已保存到数据库");
        }
    } else {
        warn!("配置管理器未初始化，无法保存到数据库");
    }

    // 重新加载全局配置包（从数据库）
    if let Err(e) = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(crate::config::reload_config_bundle())
    }) {
        warn!("重新加载配置包失败: {}", e);
        // 回退到传统的重新加载方式
        crate::config::reload_config();
    }

    info!("配置已更新，视频源删除完成");
    Ok(())
}

// 移除配置文件操作 - 配置现在完全基于数据库
#[allow(dead_code)]
fn reload_config_file() -> Result<()> {
    // 重新加载全局配置包（从数据库）
    if let Err(e) = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(crate::config::reload_config_bundle())
    }) {
        warn!("重新加载配置包失败: {}", e);
        // 回退到传统的重新加载方式
        let _new_config = crate::config::reload_config();
    }

    info!("配置已成功重新加载，新添加的视频源将在下一轮下载任务中生效");
    Ok(())
}

/// 获取当前配置
#[utoipa::path(
    get,
    path = "/api/config",
    responses(
        (status = 200, description = "获取配置成功", body = ConfigResponse),
        (status = 500, description = "服务器内部错误", body = String)
    )
)]
pub async fn get_config() -> Result<ApiResponse<crate::api::response::ConfigResponse>, ApiError> {
    // 使用配置包系统获取最新配置
    let config = crate::config::with_config(|bundle| bundle.config.clone());

    let nfo_time_type = match config.nfo_time_type {
        crate::config::NFOTimeType::FavTime => "favtime",
        crate::config::NFOTimeType::PubTime => "pubtime",
    };

    Ok(ApiResponse::ok(crate::api::response::ConfigResponse {
        video_name: config.video_name.to_string(),
        page_name: config.page_name.to_string(),
        multi_page_name: config.multi_page_name.to_string(),
        bangumi_name: config.bangumi_name.to_string(),
        folder_structure: config.folder_structure.to_string(),
        collection_folder_mode: config.collection_folder_mode.to_string(),
        time_format: config.time_format.clone(),
        interval: config.interval,
        nfo_time_type: nfo_time_type.to_string(),
        parallel_download_enabled: config.concurrent_limit.parallel_download.enabled,
        parallel_download_threads: config.concurrent_limit.parallel_download.threads,
        // 视频质量设置
        video_max_quality: format!("{:?}", config.filter_option.video_max_quality),
        video_min_quality: format!("{:?}", config.filter_option.video_min_quality),
        audio_max_quality: format!("{:?}", config.filter_option.audio_max_quality),
        audio_min_quality: format!("{:?}", config.filter_option.audio_min_quality),
        codecs: config.filter_option.codecs.iter().map(|c| format!("{}", c)).collect(),
        no_dolby_video: config.filter_option.no_dolby_video,
        no_dolby_audio: config.filter_option.no_dolby_audio,
        no_hdr: config.filter_option.no_hdr,
        no_hires: config.filter_option.no_hires,
        // 弹幕设置
        danmaku_duration: config.danmaku_option.duration,
        danmaku_font: config.danmaku_option.font.clone(),
        danmaku_font_size: config.danmaku_option.font_size,
        danmaku_width_ratio: config.danmaku_option.width_ratio,
        danmaku_horizontal_gap: config.danmaku_option.horizontal_gap,
        danmaku_lane_size: config.danmaku_option.lane_size,
        danmaku_float_percentage: config.danmaku_option.float_percentage,
        danmaku_bottom_percentage: config.danmaku_option.bottom_percentage,
        danmaku_opacity: config.danmaku_option.opacity,
        danmaku_bold: config.danmaku_option.bold,
        danmaku_outline: config.danmaku_option.outline,
        danmaku_time_offset: config.danmaku_option.time_offset,
        // 并发控制设置
        concurrent_video: config.concurrent_limit.video,
        concurrent_page: config.concurrent_limit.page,
        rate_limit: config.concurrent_limit.rate_limit.as_ref().map(|r| r.limit),
        rate_duration: config.concurrent_limit.rate_limit.as_ref().map(|r| r.duration),
        // 其他设置
        cdn_sorting: config.cdn_sorting,
        // 时区设置
        timezone: config.timezone.clone(),
        // UP主投稿风控配置
        large_submission_threshold: config.submission_risk_control.large_submission_threshold,
        base_request_delay: config.submission_risk_control.base_request_delay,
        large_submission_delay_multiplier: config.submission_risk_control.large_submission_delay_multiplier,
        enable_progressive_delay: config.submission_risk_control.enable_progressive_delay,
        max_delay_multiplier: config.submission_risk_control.max_delay_multiplier,
        enable_incremental_fetch: config.submission_risk_control.enable_incremental_fetch,
        incremental_fallback_to_full: config.submission_risk_control.incremental_fallback_to_full,
        enable_batch_processing: config.submission_risk_control.enable_batch_processing,
        batch_size: config.submission_risk_control.batch_size,
        batch_delay_seconds: config.submission_risk_control.batch_delay_seconds,
        enable_auto_backoff: config.submission_risk_control.enable_auto_backoff,
        auto_backoff_base_seconds: config.submission_risk_control.auto_backoff_base_seconds,
        auto_backoff_max_multiplier: config.submission_risk_control.auto_backoff_max_multiplier,
        scan_deleted_videos: config.scan_deleted_videos,
        // B站凭证信息
        credential: {
            let credential = config.credential.load();
            credential.as_deref().map(|cred| crate::api::response::CredentialInfo {
                sessdata: cred.sessdata.clone(),
                bili_jct: cred.bili_jct.clone(),
                buvid3: cred.buvid3.clone(),
                dedeuserid: cred.dedeuserid.clone(),
                ac_time_value: cred.ac_time_value.clone(),
            })
        },
    }))
}

/// 更新配置
#[utoipa::path(
    put,
    path = "/api/config",
    request_body = UpdateConfigRequest,
    responses(
        (status = 200, description = "配置更新成功", body = UpdateConfigResponse),
        (status = 400, description = "请求参数错误", body = String),
        (status = 500, description = "服务器内部错误", body = String)
    )
)]
pub async fn update_config(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    axum::Json(params): axum::Json<crate::api::request::UpdateConfigRequest>,
) -> Result<ApiResponse<crate::api::response::UpdateConfigResponse>, ApiError> {
    // 检查是否正在扫描
    if crate::task::is_scanning() {
        // 正在扫描，将更新配置任务加入队列
        let task_id = uuid::Uuid::new_v4().to_string();
        let update_task = crate::task::UpdateConfigTask {
            video_name: params.video_name.clone(),
            page_name: params.page_name.clone(),
            multi_page_name: params.multi_page_name.clone(),
            bangumi_name: params.bangumi_name.clone(),
            folder_structure: params.folder_structure.clone(),
            collection_folder_mode: params.collection_folder_mode.clone(),
            time_format: params.time_format.clone(),
            interval: params.interval,
            nfo_time_type: params.nfo_time_type.clone(),
            parallel_download_enabled: params.parallel_download_enabled,
            parallel_download_threads: params.parallel_download_threads,
            // 视频质量设置
            video_max_quality: params.video_max_quality.clone(),
            video_min_quality: params.video_min_quality.clone(),
            audio_max_quality: params.audio_max_quality.clone(),
            audio_min_quality: params.audio_min_quality.clone(),
            codecs: params.codecs.clone(),
            no_dolby_video: params.no_dolby_video,
            no_dolby_audio: params.no_dolby_audio,
            no_hdr: params.no_hdr,
            no_hires: params.no_hires,
            // 弹幕设置
            danmaku_duration: params.danmaku_duration,
            danmaku_font: params.danmaku_font.clone(),
            danmaku_font_size: params.danmaku_font_size,
            danmaku_width_ratio: params.danmaku_width_ratio,
            danmaku_horizontal_gap: params.danmaku_horizontal_gap,
            danmaku_lane_size: params.danmaku_lane_size,
            danmaku_float_percentage: params.danmaku_float_percentage,
            danmaku_bottom_percentage: params.danmaku_bottom_percentage,
            danmaku_opacity: params.danmaku_opacity,
            danmaku_bold: params.danmaku_bold,
            danmaku_outline: params.danmaku_outline,
            danmaku_time_offset: params.danmaku_time_offset,
            // 并发控制设置
            concurrent_video: params.concurrent_video,
            concurrent_page: params.concurrent_page,
            rate_limit: params.rate_limit,
            rate_duration: params.rate_duration,
            // 其他设置
            cdn_sorting: params.cdn_sorting,
            // 时区设置
            timezone: params.timezone.clone(),
            // UP主投稿风控配置
            large_submission_threshold: params.large_submission_threshold,
            base_request_delay: params.base_request_delay,
            large_submission_delay_multiplier: params.large_submission_delay_multiplier,
            enable_progressive_delay: params.enable_progressive_delay,
            max_delay_multiplier: params.max_delay_multiplier,
            enable_incremental_fetch: params.enable_incremental_fetch,
            incremental_fallback_to_full: params.incremental_fallback_to_full,
            enable_batch_processing: params.enable_batch_processing,
            batch_size: params.batch_size,
            batch_delay_seconds: params.batch_delay_seconds,
            enable_auto_backoff: params.enable_auto_backoff,
            auto_backoff_base_seconds: params.auto_backoff_base_seconds,
            auto_backoff_max_multiplier: params.auto_backoff_max_multiplier,
            task_id: task_id.clone(),
        };

        crate::task::enqueue_update_task(update_task, &db).await?;

        info!("检测到正在扫描，更新配置任务已加入队列等待处理");

        return Ok(ApiResponse::ok(crate::api::response::UpdateConfigResponse {
            success: true,
            message: "正在扫描中，更新配置任务已加入队列，将在扫描完成后自动处理".to_string(),
            updated_files: None,
        }));
    }

    // 没有扫描，直接执行更新配置
    match update_config_internal(db, params).await {
        Ok(response) => Ok(ApiResponse::ok(response)),
        Err(e) => Err(e),
    }
}

/// 内部更新配置函数（用于队列处理和直接调用）
pub async fn update_config_internal(
    db: Arc<DatabaseConnection>,
    params: crate::api::request::UpdateConfigRequest,
) -> Result<crate::api::response::UpdateConfigResponse, ApiError> {
    use std::borrow::Cow;

    // 获取当前配置的副本
    let mut config = crate::config::reload_config();
    let mut updated_fields = Vec::new();

    // 记录原始的NFO时间类型，用于比较是否真正发生了变化
    let original_nfo_time_type = config.nfo_time_type.clone();

    // 记录原始的命名相关配置，用于比较是否真正发生了变化
    let original_video_name = config.video_name.clone();
    let original_page_name = config.page_name.clone();
    let original_multi_page_name = config.multi_page_name.clone();
    let original_bangumi_name = config.bangumi_name.clone();
    let original_folder_structure = config.folder_structure.clone();
    let original_collection_folder_mode = config.collection_folder_mode.clone();

    // 更新配置字段
    if let Some(video_name) = params.video_name {
        if !video_name.trim().is_empty() && video_name != original_video_name.as_ref() {
            config.video_name = Cow::Owned(video_name);
            updated_fields.push("video_name");
        }
    }

    if let Some(page_name) = params.page_name {
        if !page_name.trim().is_empty() && page_name != original_page_name.as_ref() {
            config.page_name = Cow::Owned(page_name);
            updated_fields.push("page_name");
        }
    }

    if let Some(multi_page_name) = params.multi_page_name {
        if !multi_page_name.trim().is_empty() && multi_page_name != original_multi_page_name.as_ref() {
            config.multi_page_name = Cow::Owned(multi_page_name);
            updated_fields.push("multi_page_name");
        }
    }

    if let Some(folder_structure) = params.folder_structure {
        if !folder_structure.trim().is_empty() && folder_structure != original_folder_structure.as_ref() {
            config.folder_structure = Cow::Owned(folder_structure);
            updated_fields.push("folder_structure");
        }
    }

    if let Some(collection_folder_mode) = params.collection_folder_mode {
        if !collection_folder_mode.trim().is_empty()
            && collection_folder_mode != original_collection_folder_mode.as_ref()
        {
            // 验证合集文件夹模式的有效性
            match collection_folder_mode.as_str() {
                "separate" | "unified" => {
                    config.collection_folder_mode = Cow::Owned(collection_folder_mode);
                    updated_fields.push("collection_folder_mode");
                }
                _ => {
                    return Err(
                        anyhow!("无效的合集文件夹模式，只支持 'separate'（分离模式）或 'unified'（统一模式）").into(),
                    )
                }
            }
        }
    }

    if let Some(time_format) = params.time_format {
        if !time_format.trim().is_empty() && time_format != config.time_format {
            config.time_format = time_format;
            updated_fields.push("time_format");
        }
    }

    if let Some(interval) = params.interval {
        if interval > 0 && interval != config.interval {
            config.interval = interval;
            updated_fields.push("interval");
        }
    }

    if let Some(nfo_time_type) = params.nfo_time_type {
        let new_nfo_time_type = match nfo_time_type.as_str() {
            "favtime" => crate::config::NFOTimeType::FavTime,
            "pubtime" => crate::config::NFOTimeType::PubTime,
            _ => return Err(anyhow!("无效的NFO时间类型，只支持 'favtime' 或 'pubtime'").into()),
        };

        // 只有当NFO时间类型真正发生变化时才标记为需要更新
        if original_nfo_time_type != new_nfo_time_type {
            config.nfo_time_type = new_nfo_time_type;
            updated_fields.push("nfo_time_type");
        }
    }

    if let Some(bangumi_name) = params.bangumi_name {
        if !bangumi_name.trim().is_empty() && bangumi_name != original_bangumi_name.as_ref() {
            config.bangumi_name = Cow::Owned(bangumi_name);
            updated_fields.push("bangumi_name");
        }
    }

    // 处理多线程下载配置
    if let Some(enabled) = params.parallel_download_enabled {
        if enabled != config.concurrent_limit.parallel_download.enabled {
            config.concurrent_limit.parallel_download.enabled = enabled;
            updated_fields.push("parallel_download_enabled");
        }
    }

    if let Some(threads) = params.parallel_download_threads {
        if threads > 0 && threads != config.concurrent_limit.parallel_download.threads {
            config.concurrent_limit.parallel_download.threads = threads;
            updated_fields.push("parallel_download_threads");
        }
    }

    // 处理视频质量设置
    if let Some(quality) = params.video_max_quality {
        use crate::bilibili::VideoQuality;
        if let Ok(new_quality) = quality.parse::<VideoQuality>() {
            if new_quality != config.filter_option.video_max_quality {
                config.filter_option.video_max_quality = new_quality;
                updated_fields.push("video_max_quality");
            }
        }
    }

    if let Some(quality) = params.video_min_quality {
        use crate::bilibili::VideoQuality;
        if let Ok(new_quality) = quality.parse::<VideoQuality>() {
            if new_quality != config.filter_option.video_min_quality {
                config.filter_option.video_min_quality = new_quality;
                updated_fields.push("video_min_quality");
            }
        }
    }

    if let Some(quality) = params.audio_max_quality {
        use crate::bilibili::AudioQuality;
        if let Ok(new_quality) = quality.parse::<AudioQuality>() {
            if new_quality != config.filter_option.audio_max_quality {
                config.filter_option.audio_max_quality = new_quality;
                updated_fields.push("audio_max_quality");
            }
        }
    }

    if let Some(quality) = params.audio_min_quality {
        use crate::bilibili::AudioQuality;
        if let Ok(new_quality) = quality.parse::<AudioQuality>() {
            if new_quality != config.filter_option.audio_min_quality {
                config.filter_option.audio_min_quality = new_quality;
                updated_fields.push("audio_min_quality");
            }
        }
    }

    if let Some(codecs) = params.codecs {
        use crate::bilibili::VideoCodecs;
        let mut new_codecs = Vec::new();
        for codec_str in codecs {
            if let Ok(codec) = codec_str.parse::<VideoCodecs>() {
                new_codecs.push(codec);
            }
        }
        if !new_codecs.is_empty() && new_codecs != config.filter_option.codecs {
            config.filter_option.codecs = new_codecs;
            updated_fields.push("codecs");
        }
    }

    if let Some(no_dolby_video) = params.no_dolby_video {
        if no_dolby_video != config.filter_option.no_dolby_video {
            config.filter_option.no_dolby_video = no_dolby_video;
            updated_fields.push("no_dolby_video");
        }
    }

    if let Some(no_dolby_audio) = params.no_dolby_audio {
        if no_dolby_audio != config.filter_option.no_dolby_audio {
            config.filter_option.no_dolby_audio = no_dolby_audio;
            updated_fields.push("no_dolby_audio");
        }
    }

    if let Some(no_hdr) = params.no_hdr {
        if no_hdr != config.filter_option.no_hdr {
            config.filter_option.no_hdr = no_hdr;
            updated_fields.push("no_hdr");
        }
    }

    if let Some(no_hires) = params.no_hires {
        if no_hires != config.filter_option.no_hires {
            config.filter_option.no_hires = no_hires;
            updated_fields.push("no_hires");
        }
    }

    // 处理弹幕设置
    if let Some(duration) = params.danmaku_duration {
        if duration != config.danmaku_option.duration {
            config.danmaku_option.duration = duration;
            updated_fields.push("danmaku_duration");
        }
    }

    if let Some(font) = params.danmaku_font {
        if !font.trim().is_empty() && font != config.danmaku_option.font {
            config.danmaku_option.font = font;
            updated_fields.push("danmaku_font");
        }
    }

    if let Some(font_size) = params.danmaku_font_size {
        if font_size != config.danmaku_option.font_size {
            config.danmaku_option.font_size = font_size;
            updated_fields.push("danmaku_font_size");
        }
    }

    if let Some(width_ratio) = params.danmaku_width_ratio {
        if width_ratio != config.danmaku_option.width_ratio {
            config.danmaku_option.width_ratio = width_ratio;
            updated_fields.push("danmaku_width_ratio");
        }
    }

    if let Some(horizontal_gap) = params.danmaku_horizontal_gap {
        if horizontal_gap != config.danmaku_option.horizontal_gap {
            config.danmaku_option.horizontal_gap = horizontal_gap;
            updated_fields.push("danmaku_horizontal_gap");
        }
    }

    if let Some(lane_size) = params.danmaku_lane_size {
        if lane_size != config.danmaku_option.lane_size {
            config.danmaku_option.lane_size = lane_size;
            updated_fields.push("danmaku_lane_size");
        }
    }

    if let Some(float_percentage) = params.danmaku_float_percentage {
        if float_percentage != config.danmaku_option.float_percentage {
            config.danmaku_option.float_percentage = float_percentage;
            updated_fields.push("danmaku_float_percentage");
        }
    }

    if let Some(bottom_percentage) = params.danmaku_bottom_percentage {
        if bottom_percentage != config.danmaku_option.bottom_percentage {
            config.danmaku_option.bottom_percentage = bottom_percentage;
            updated_fields.push("danmaku_bottom_percentage");
        }
    }

    if let Some(opacity) = params.danmaku_opacity {
        if opacity != config.danmaku_option.opacity {
            config.danmaku_option.opacity = opacity;
            updated_fields.push("danmaku_opacity");
        }
    }

    if let Some(bold) = params.danmaku_bold {
        if bold != config.danmaku_option.bold {
            config.danmaku_option.bold = bold;
            updated_fields.push("danmaku_bold");
        }
    }

    if let Some(outline) = params.danmaku_outline {
        if outline != config.danmaku_option.outline {
            config.danmaku_option.outline = outline;
            updated_fields.push("danmaku_outline");
        }
    }

    if let Some(time_offset) = params.danmaku_time_offset {
        if time_offset != config.danmaku_option.time_offset {
            config.danmaku_option.time_offset = time_offset;
            updated_fields.push("danmaku_time_offset");
        }
    }

    // 处理并发控制设置
    if let Some(concurrent_video) = params.concurrent_video {
        if concurrent_video > 0 && concurrent_video != config.concurrent_limit.video {
            config.concurrent_limit.video = concurrent_video;
            updated_fields.push("concurrent_video");
        }
    }

    if let Some(concurrent_page) = params.concurrent_page {
        if concurrent_page > 0 && concurrent_page != config.concurrent_limit.page {
            config.concurrent_limit.page = concurrent_page;
            updated_fields.push("concurrent_page");
        }
    }

    if let Some(rate_limit) = params.rate_limit {
        if rate_limit > 0 {
            let current_limit = config
                .concurrent_limit
                .rate_limit
                .as_ref()
                .map(|r| r.limit)
                .unwrap_or(0);
            if rate_limit != current_limit {
                if let Some(ref mut rate) = config.concurrent_limit.rate_limit {
                    rate.limit = rate_limit;
                } else {
                    config.concurrent_limit.rate_limit = Some(crate::config::RateLimit {
                        limit: rate_limit,
                        duration: 250, // 默认值
                    });
                }
                updated_fields.push("rate_limit");
            }
        }
    }

    if let Some(rate_duration) = params.rate_duration {
        if rate_duration > 0 {
            let current_duration = config
                .concurrent_limit
                .rate_limit
                .as_ref()
                .map(|r| r.duration)
                .unwrap_or(0);
            if rate_duration != current_duration {
                if let Some(ref mut rate) = config.concurrent_limit.rate_limit {
                    rate.duration = rate_duration;
                } else {
                    config.concurrent_limit.rate_limit = Some(crate::config::RateLimit {
                        limit: 4, // 默认值
                        duration: rate_duration,
                    });
                }
                updated_fields.push("rate_duration");
            }
        }
    }

    // 处理其他设置
    if let Some(cdn_sorting) = params.cdn_sorting {
        if cdn_sorting != config.cdn_sorting {
            config.cdn_sorting = cdn_sorting;
            updated_fields.push("cdn_sorting");
        }
    }

    // 处理时区设置
    if let Some(timezone) = params.timezone {
        if !timezone.trim().is_empty() && timezone != config.timezone {
            let old_timezone = config.timezone.clone();
            config.timezone = timezone.clone();
            updated_fields.push("timezone");

            // 同步数据库中的时间戳到新时区
            info!(
                "时区配置已更新，开始同步数据库时间戳从 {} 到 {}",
                old_timezone, timezone
            );
            match sync_database_timestamps(db.clone(), &old_timezone, &timezone).await {
                Ok(count) => {
                    info!("数据库时间戳同步完成，共更新了 {} 条记录", count);
                }
                Err(e) => {
                    error!("数据库时间戳同步失败: {}", e);
                    // 即使同步失败，配置更新仍然成功
                }
            }
        }
    }

    // 处理显示已删除视频配置
    if let Some(scan_deleted) = params.scan_deleted_videos {
        if scan_deleted != config.scan_deleted_videos {
            config.scan_deleted_videos = scan_deleted;
            updated_fields.push("scan_deleted_videos");
        }
    }

    // 处理UP主投稿风控配置
    if let Some(threshold) = params.large_submission_threshold {
        if threshold != config.submission_risk_control.large_submission_threshold {
            config.submission_risk_control.large_submission_threshold = threshold;
            updated_fields.push("large_submission_threshold");
        }
    }

    if let Some(delay) = params.base_request_delay {
        if delay != config.submission_risk_control.base_request_delay {
            config.submission_risk_control.base_request_delay = delay;
            updated_fields.push("base_request_delay");
        }
    }

    if let Some(multiplier) = params.large_submission_delay_multiplier {
        if multiplier != config.submission_risk_control.large_submission_delay_multiplier {
            config.submission_risk_control.large_submission_delay_multiplier = multiplier;
            updated_fields.push("large_submission_delay_multiplier");
        }
    }

    if let Some(enabled) = params.enable_progressive_delay {
        if enabled != config.submission_risk_control.enable_progressive_delay {
            config.submission_risk_control.enable_progressive_delay = enabled;
            updated_fields.push("enable_progressive_delay");
        }
    }

    if let Some(multiplier) = params.max_delay_multiplier {
        if multiplier != config.submission_risk_control.max_delay_multiplier {
            config.submission_risk_control.max_delay_multiplier = multiplier;
            updated_fields.push("max_delay_multiplier");
        }
    }

    if let Some(enabled) = params.enable_incremental_fetch {
        if enabled != config.submission_risk_control.enable_incremental_fetch {
            config.submission_risk_control.enable_incremental_fetch = enabled;
            updated_fields.push("enable_incremental_fetch");
        }
    }

    if let Some(enabled) = params.incremental_fallback_to_full {
        if enabled != config.submission_risk_control.incremental_fallback_to_full {
            config.submission_risk_control.incremental_fallback_to_full = enabled;
            updated_fields.push("incremental_fallback_to_full");
        }
    }

    if let Some(enabled) = params.enable_batch_processing {
        if enabled != config.submission_risk_control.enable_batch_processing {
            config.submission_risk_control.enable_batch_processing = enabled;
            updated_fields.push("enable_batch_processing");
        }
    }

    if let Some(size) = params.batch_size {
        if size != config.submission_risk_control.batch_size {
            config.submission_risk_control.batch_size = size;
            updated_fields.push("batch_size");
        }
    }

    if let Some(delay) = params.batch_delay_seconds {
        if delay != config.submission_risk_control.batch_delay_seconds {
            config.submission_risk_control.batch_delay_seconds = delay;
            updated_fields.push("batch_delay_seconds");
        }
    }

    if let Some(enabled) = params.enable_auto_backoff {
        if enabled != config.submission_risk_control.enable_auto_backoff {
            config.submission_risk_control.enable_auto_backoff = enabled;
            updated_fields.push("enable_auto_backoff");
        }
    }

    if let Some(seconds) = params.auto_backoff_base_seconds {
        if seconds != config.submission_risk_control.auto_backoff_base_seconds {
            config.submission_risk_control.auto_backoff_base_seconds = seconds;
            updated_fields.push("auto_backoff_base_seconds");
        }
    }

    if let Some(multiplier) = params.auto_backoff_max_multiplier {
        if multiplier != config.submission_risk_control.auto_backoff_max_multiplier {
            config.submission_risk_control.auto_backoff_max_multiplier = multiplier;
            updated_fields.push("auto_backoff_max_multiplier");
        }
    }

    if updated_fields.is_empty() {
        return Ok(crate::api::response::UpdateConfigResponse {
            success: false,
            message: "没有提供有效的配置更新".to_string(),
            updated_files: None,
        });
    }

    // 移除配置文件保存 - 配置现在完全基于数据库
    // config.save()?;

    // 保存配置到数据库
    {
        use crate::config::ConfigManager;
        let manager = ConfigManager::new(db.as_ref().clone());
        if let Err(e) = manager.save_config(&config).await {
            warn!("保存配置到数据库失败: {}", e);
        } else {
            info!("配置已保存到数据库");
        }
    }

    // 重新加载全局配置包（从数据库）
    if let Err(e) = crate::config::reload_config_bundle().await {
        warn!("重新加载配置包失败: {}", e);
        // 回退到传统的重新加载方式
        crate::config::reload_config();
    }

    // 如果更新了命名相关的配置，重命名已下载的文件
    let mut updated_files = 0u32;
    let naming_fields = [
        "video_name",
        "page_name",
        "multi_page_name",
        "bangumi_name",
        "folder_structure",
    ];
    let should_rename = updated_fields.iter().any(|field| naming_fields.contains(field));

    if should_rename {
        // 暂停定时扫描任务，避免与重命名操作产生数据库锁定冲突
        crate::task::pause_scanning().await;
        info!("重命名操作开始，已暂停定时扫描任务");

        // 根据更新的字段类型来决定重命名哪些文件
        let rename_single_page = updated_fields.contains(&"page_name") || updated_fields.contains(&"video_name");
        let rename_multi_page = updated_fields.contains(&"multi_page_name") || updated_fields.contains(&"video_name");
        let rename_bangumi = updated_fields.contains(&"bangumi_name") || updated_fields.contains(&"video_name");
        let rename_folder_structure = updated_fields.contains(&"folder_structure");

        // 执行文件重命名并等待完成
        match rename_existing_files(
            db.clone(),
            &config,
            rename_single_page,
            rename_multi_page,
            rename_bangumi,
            rename_folder_structure,
        )
        .await
        {
            Ok(count) => {
                updated_files = count;
                info!("重命名操作完成，共处理了 {} 个文件/文件夹", count);
            }
            Err(e) => {
                error!("重命名已下载文件时出错: {}", e);
                // 即使重命名失败，配置更新仍然成功
            }
        }

        // 恢复定时扫描任务
        crate::task::resume_scanning();
        info!("重命名操作结束，已恢复定时扫描任务");
    }

    // 检查是否需要重新生成NFO文件
    let should_regenerate_nfo = updated_fields.contains(&"nfo_time_type");

    if should_regenerate_nfo {
        // 重新生成NFO文件
        match regenerate_nfo_files(db.clone(), &config).await {
            Ok(count) => {
                if !should_rename {
                    updated_files = count;
                }
            }
            Err(e) => {
                error!("重新生成NFO文件时出错: {}", e);
                // 即使重新生成失败，配置更新仍然成功
            }
        }
    }

    Ok(crate::api::response::UpdateConfigResponse {
        success: true,
        message: if should_rename && should_regenerate_nfo {
            format!(
                "配置更新成功，已更新字段: {}，重命名了文件并重新生成了NFO文件",
                updated_fields.join(", ")
            )
        } else if should_rename {
            format!(
                "配置更新成功，已更新字段: {}，重命名了 {} 个文件/文件夹",
                updated_fields.join(", "),
                updated_files
            )
        } else if should_regenerate_nfo {
            format!(
                "配置更新成功，已更新字段: {}，重新生成了 {} 个NFO文件",
                updated_fields.join(", "),
                updated_files
            )
        } else {
            format!("配置更新成功，已更新字段: {}", updated_fields.join(", "))
        },
        updated_files: if should_rename || should_regenerate_nfo {
            Some(updated_files)
        } else {
            None
        },
    })
}

/// 查找分页文件的原始命名模式
fn find_page_file_pattern(video_path: &std::path::Path, page: &bili_sync_entity::page::Model) -> Result<String> {
    if !video_path.exists() {
        return Ok(String::new());
    }

    if let Ok(entries) = std::fs::read_dir(video_path) {
        for entry in entries.flatten() {
            let file_path = entry.path();
            let file_name = file_path.file_name().unwrap_or_default().to_string_lossy();

            // 尝试通过文件名中的分页编号来匹配主文件（MP4）
            if file_name.ends_with(".mp4")
                && (file_name.contains(&format!("{:02}", page.pid))
                    || file_name.contains(&format!("{:03}", page.pid))
                    || file_name.contains(&page.name))
            {
                // 找到MP4文件，提取文件名（不包括扩展名）
                if let Some(file_stem) = file_path.file_stem() {
                    return Ok(file_stem.to_string_lossy().to_string());
                }
            }
        }
    }

    Ok(String::new())
}

/// 重命名已下载的文件以匹配新的命名规则
#[allow(unused_variables)] // rename_folder_structure 参数表示是否更新了 folder_structure 配置，虽然当前未使用但保留以备将来扩展
async fn rename_existing_files(
    db: Arc<DatabaseConnection>,
    config: &crate::config::Config,
    rename_single_page: bool,
    rename_multi_page: bool,
    rename_bangumi: bool,
    rename_folder_structure: bool,
) -> Result<u32> {
    use handlebars::{handlebars_helper, Handlebars};
    use sea_orm::*;
    use std::path::Path;

    info!("开始重命名已下载的文件以匹配新的配置...");

    let mut updated_count = 0u32;

    // 创建模板引擎
    let mut handlebars = Handlebars::new();

    // **关键修复：注册所有必要的helper函数，确保与下载时使用相同的模板引擎功能**
    handlebars_helper!(truncate: |s: String, len: usize| {
        if s.chars().count() > len {
            s.chars().take(len).collect::<String>()
        } else {
            s.to_string()
        }
    });
    handlebars.register_helper("truncate", Box::new(truncate));

    // 使用register_template_string而不是path_safe_register来避免生命周期问题
    // 同时处理正斜杠和反斜杠，确保跨平台兼容性
    let video_template = config.video_name.replace(['/', '\\'], "__SEP__");
    let page_template = config.page_name.replace(['/', '\\'], "__SEP__");

    handlebars.register_template_string("video", video_template)?;
    handlebars.register_template_string("page", page_template)?;

    // 分别处理不同类型的视频
    let mut all_videos = Vec::new();

    // 1. 处理非番剧类型的视频（原有逻辑）
    if rename_single_page || rename_multi_page {
        let regular_videos = bili_sync_entity::video::Entity::find()
            .filter(bili_sync_entity::video::Column::DownloadStatus.gt(0))
            .filter(
                // 排除番剧类型（source_type=1），包含其他所有类型
                bili_sync_entity::video::Column::SourceType
                    .is_null()
                    .or(bili_sync_entity::video::Column::SourceType.ne(1)),
            )
            .all(db.as_ref())
            .await?;
        all_videos.extend(regular_videos);
    }

    // 2. 处理番剧类型的视频
    if rename_bangumi {
        let bangumi_videos = bili_sync_entity::video::Entity::find()
            .filter(bili_sync_entity::video::Column::DownloadStatus.gt(0))
            .filter(bili_sync_entity::video::Column::SourceType.eq(1)) // 番剧类型
            .all(db.as_ref())
            .await?;
        all_videos.extend(bangumi_videos);
    }

    info!("找到 {} 个需要检查的视频", all_videos.len());

    for video in all_videos {
        // 检查视频类型，决定是否需要重命名
        let is_single_page = video.single_page.unwrap_or(true);
        let is_bangumi = video.source_type == Some(1);

        // 根据视频类型和配置更新情况决定是否跳过
        let should_process_video = if is_bangumi {
            rename_bangumi // 番剧视频只在bangumi_name或video_name更新时处理
        } else if is_single_page {
            rename_single_page // 单P视频只在page_name或video_name更新时处理
        } else {
            rename_multi_page // 多P视频只在multi_page_name或video_name更新时处理
        };

        if !should_process_video {
            let video_type = if is_bangumi {
                "番剧"
            } else if is_single_page {
                "单P"
            } else {
                "多P"
            };
            debug!("跳过视频重命名: {} (类型: {})", video.name, video_type);
            continue;
        }

        // 构建模板数据
        let mut template_data = std::collections::HashMap::new();
        template_data.insert("title".to_string(), serde_json::Value::String(video.name.clone()));
        template_data.insert("show_title".to_string(), serde_json::Value::String(video.name.clone()));
        template_data.insert("bvid".to_string(), serde_json::Value::String(video.bvid.clone()));
        template_data.insert(
            "upper_name".to_string(),
            serde_json::Value::String(video.upper_name.clone()),
        );
        template_data.insert(
            "upper_mid".to_string(),
            serde_json::Value::String(video.upper_id.to_string()),
        );

        // 格式化时间
        let formatted_pubtime = video.pubtime.format(&config.time_format).to_string();
        template_data.insert(
            "pubtime".to_string(),
            serde_json::Value::String(formatted_pubtime.clone()),
        );

        let formatted_favtime = video.favtime.format(&config.time_format).to_string();
        template_data.insert("fav_time".to_string(), serde_json::Value::String(formatted_favtime));

        let formatted_ctime = video.ctime.format(&config.time_format).to_string();
        template_data.insert("ctime".to_string(), serde_json::Value::String(formatted_ctime));

        // 确定最终的视频文件夹路径
        let final_video_path = if is_bangumi {
            // 番剧不重命名视频文件夹，直接使用现有路径
            let video_path = Path::new(&video.path);
            if video_path.exists() {
                video_path.to_path_buf()
            } else {
                // 如果路径不存在，尝试智能查找
                if let Some(parent_dir) = video_path.parent() {
                    if let Ok(entries) = std::fs::read_dir(parent_dir) {
                        let mut found_path = None;
                        for entry in entries.flatten() {
                            let entry_path = entry.path();
                            if entry_path.is_dir() {
                                let dir_name = entry_path.file_name().unwrap_or_default().to_string_lossy();
                                // 检查是否包含视频的bvid或标题
                                if dir_name.contains(&video.bvid) || dir_name.contains(&video.name) {
                                    found_path = Some(entry_path);
                                    break;
                                }
                            }
                        }
                        found_path.unwrap_or_else(|| video_path.to_path_buf())
                    } else {
                        video_path.to_path_buf()
                    }
                } else {
                    video_path.to_path_buf()
                }
            }
        } else {
            // 非番剧视频的重命名逻辑（改进的智能重组逻辑）
            // 渲染新的视频文件夹名称（使用video_name模板）
            let template_value = serde_json::Value::Object(template_data.clone().into_iter().collect());
            let rendered_name = handlebars
                .render("video", &template_value)
                .unwrap_or_else(|_| video.name.clone());
            let base_video_name =
                crate::utils::filenamify::filenamify(&rendered_name).replace("__SEP__", std::path::MAIN_SEPARATOR_STR);

            // 使用视频记录中的路径信息
            let video_path = Path::new(&video.path);
            if let Some(parent_dir) = video_path.parent() {
                // **智能判断：根据模板内容决定是否需要去重**
                // 如果模板包含会产生相同名称的变量（如upper_name），则不使用智能去重
                // 如果模板包含会产生不同名称的变量（如title），则使用智能去重避免冲突
                let video_template = config.video_name.as_ref();
                let needs_deduplication = video_template.contains("title")
                    || video_template.contains("name") && !video_template.contains("upper_name");

                let expected_new_path = if needs_deduplication {
                    // 使用智能去重生成唯一文件夹名
                    let unique_folder_name =
                        generate_unique_folder_name(parent_dir, &base_video_name, &video.bvid, &formatted_pubtime);
                    parent_dir.join(&unique_folder_name)
                } else {
                    // 不使用去重，允许多个视频共享同一文件夹
                    parent_dir.join(&base_video_name)
                };

                // **修复分离逻辑：从合并文件夹中提取单个视频的文件**
                // 智能查找包含此视频文件的文件夹
                let source_folder_with_files = if video_path.exists() {
                    Some(video_path.to_path_buf())
                } else {
                    // 在父目录中查找包含此视频文件的文件夹
                    if let Ok(entries) = std::fs::read_dir(parent_dir) {
                        let mut found_path = None;
                        for entry in entries.flatten() {
                            let entry_path = entry.path();
                            if entry_path.is_dir() {
                                // 检查文件夹内是否包含属于此视频的文件
                                if let Ok(files) = std::fs::read_dir(&entry_path) {
                                    for file_entry in files.flatten() {
                                        let file_name_os = file_entry.file_name();
                                        let file_name = file_name_os.to_string_lossy();
                                        // 通过bvid匹配文件
                                        if file_name.contains(&video.bvid) {
                                            found_path = Some(entry_path.clone());
                                            break;
                                        }
                                    }
                                    if found_path.is_some() {
                                        break;
                                    }
                                }
                            }
                        }
                        found_path
                    } else {
                        None
                    }
                };

                // 处理文件提取和移动的情况
                if let Some(source_path) = source_folder_with_files {
                    if source_path != expected_new_path {
                        // 需要从源文件夹中提取属于此视频的文件
                        match extract_video_files_by_database(db.as_ref(), video.id, &expected_new_path).await {
                            Ok(_) => {
                                info!(
                                    "从共享文件夹提取视频文件成功: {:?} -> {:?} (bvid: {})",
                                    source_path, expected_new_path, video.bvid
                                );
                                updated_count += 1;
                                expected_new_path.clone()
                            }
                            Err(e) => {
                                warn!(
                                    "从共享文件夹提取视频文件失败: {:?} -> {:?}, 错误: {}",
                                    source_path, expected_new_path, e
                                );
                                source_path.clone()
                            }
                        }
                    } else {
                        // 文件夹已经是正确的名称和位置
                        source_path.clone()
                    }
                } else {
                    // 文件夹不存在，使用新路径
                    expected_new_path.clone()
                }
            } else {
                video_path.to_path_buf()
            }
        };

        // **关键修复：始终更新数据库中的路径记录**
        // 不管文件夹是否重命名，都要确保数据库路径与实际文件系统路径一致
        let final_path_str = final_video_path.to_string_lossy().to_string();
        if video.path != final_path_str {
            let mut video_update: bili_sync_entity::video::ActiveModel = video.clone().into();
            video_update.path = Set(final_path_str.clone());
            if let Err(e) = video_update.update(db.as_ref()).await {
                warn!("更新数据库中的视频路径失败: {}", e);
            } else {
                debug!("更新数据库视频路径: {} -> {}", video.path, final_path_str);
            }
        }

        // **新增：处理视频级别的文件重命名（poster、fanart、nfo）**
        // 只对非番剧的多P视频进行视频级别文件重命名
        if !is_single_page && !is_bangumi {
            // 多P视频需要重命名视频级别的文件
            let old_video_name = Path::new(&video.path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| video.name.clone());

            let new_video_name = final_video_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| video.name.clone());

            if old_video_name != new_video_name {
                // 重命名视频级别的文件
                let video_level_files = [
                    (
                        format!("{}-poster.jpg", old_video_name),
                        format!("{}-poster.jpg", new_video_name),
                    ),
                    (
                        format!("{}-fanart.jpg", old_video_name),
                        format!("{}-fanart.jpg", new_video_name),
                    ),
                    (format!("{}.nfo", old_video_name), format!("{}.nfo", new_video_name)),
                    // 兼容旧的硬编码文件名
                    ("poster.jpg".to_string(), format!("{}-poster.jpg", new_video_name)),
                    ("fanart.jpg".to_string(), format!("{}-fanart.jpg", new_video_name)),
                    ("tvshow.nfo".to_string(), format!("{}.nfo", new_video_name)),
                ];

                for (old_file_name, new_file_name) in video_level_files {
                    let old_file_path = final_video_path.join(&old_file_name);
                    let new_file_path = final_video_path.join(&new_file_name);

                    if old_file_path.exists() && old_file_path != new_file_path {
                        // **关键修复：检查目标文件是否已存在，避免覆盖**
                        let final_new_file_path = if new_file_path.exists() {
                            // 目标文件已存在，生成唯一文件名避免覆盖
                            let file_stem = new_file_path.file_stem().unwrap_or_default().to_string_lossy();
                            let file_extension = new_file_path.extension().unwrap_or_default().to_string_lossy();
                            let parent_dir = new_file_path.parent().unwrap_or(&final_video_path);

                            // 尝试添加BV号后缀避免冲突
                            let bvid_suffix = &video.bvid;
                            let unique_name = if file_extension.is_empty() {
                                format!("{}-{}", file_stem, bvid_suffix)
                            } else {
                                format!("{}-{}.{}", file_stem, bvid_suffix, file_extension)
                            };
                            let unique_path = parent_dir.join(unique_name);

                            // 如果BV号后缀仍然冲突，使用时间戳
                            if unique_path.exists() {
                                let timestamp = chrono::Local::now().format("%H%M%S").to_string();
                                let final_name = if file_extension.is_empty() {
                                    format!("{}-{}-{}", file_stem, bvid_suffix, timestamp)
                                } else {
                                    format!("{}-{}-{}.{}", file_stem, bvid_suffix, timestamp, file_extension)
                                };
                                parent_dir.join(final_name)
                            } else {
                                unique_path
                            }
                        } else {
                            new_file_path.clone()
                        };

                        match std::fs::rename(&old_file_path, &final_new_file_path) {
                            Ok(_) => {
                                if final_new_file_path != new_file_path {
                                    warn!(
                                        "检测到视频级别文件名冲突，已重命名避免覆盖: {:?} -> {:?}",
                                        old_file_path, final_new_file_path
                                    );
                                } else {
                                    debug!(
                                        "重命名视频级别文件成功: {:?} -> {:?}",
                                        old_file_path, final_new_file_path
                                    );
                                }
                                updated_count += 1;
                            }
                            Err(e) => {
                                warn!(
                                    "重命名视频级别文件失败: {:?} -> {:?}, 错误: {}",
                                    old_file_path, final_new_file_path, e
                                );
                            }
                        }
                    }
                }
            }
        }

        // 处理分页视频的重命名
        let pages = bili_sync_entity::page::Entity::find()
            .filter(bili_sync_entity::page::Column::VideoId.eq(video.id))
            .filter(bili_sync_entity::page::Column::DownloadStatus.gt(0))
            .all(db.as_ref())
            .await?;

        for page in pages {
            // 为分页添加额外的模板数据
            let mut page_template_data = template_data.clone();
            page_template_data.insert("ptitle".to_string(), serde_json::Value::String(page.name.clone()));
            page_template_data.insert("pid".to_string(), serde_json::Value::String(page.pid.to_string()));
            page_template_data.insert(
                "pid_pad".to_string(),
                serde_json::Value::String(format!("{:02}", page.pid)),
            );

            // 为多P视频和番剧添加season相关变量
            if !is_single_page || is_bangumi {
                let season_number = if is_bangumi {
                    video.season_number.unwrap_or(1)
                } else {
                    1
                };
                let episode_number = if is_bangumi {
                    video.episode_number.unwrap_or(page.pid)
                } else {
                    page.pid
                };

                page_template_data.insert(
                    "season".to_string(),
                    serde_json::Value::String(season_number.to_string()),
                );
                page_template_data.insert(
                    "season_pad".to_string(),
                    serde_json::Value::String(format!("{:02}", season_number)),
                );
                page_template_data.insert("pid".to_string(), serde_json::Value::String(episode_number.to_string()));
                page_template_data.insert(
                    "pid_pad".to_string(),
                    serde_json::Value::String(format!("{:02}", episode_number)),
                );
            }

            page_template_data.insert(
                "duration".to_string(),
                serde_json::Value::String(page.duration.to_string()),
            );

            if let Some(width) = page.width {
                page_template_data.insert("width".to_string(), serde_json::Value::String(width.to_string()));
            }

            if let Some(height) = page.height {
                page_template_data.insert("height".to_string(), serde_json::Value::String(height.to_string()));
            }

            // 根据视频类型选择不同的模板
            let page_template_value = serde_json::Value::Object(page_template_data.into_iter().collect());
            let rendered_page_name = if is_bangumi {
                // 番剧使用bangumi_name模板
                match handlebars.render_template(&config.bangumi_name, &page_template_value) {
                    Ok(rendered) => rendered,
                    Err(_) => {
                        // 如果渲染失败，使用默认番剧格式
                        let season_number = video.season_number.unwrap_or(1);
                        let episode_number = video.episode_number.unwrap_or(page.pid);
                        format!("S{:02}E{:02}-{:02}", season_number, episode_number, episode_number)
                    }
                }
            } else if is_single_page {
                // 单P视频使用page_name模板
                match handlebars.render("page", &page_template_value) {
                    Ok(rendered) => {
                        debug!("单P视频模板渲染成功: '{}' -> '{}'", config.page_name, rendered);
                        rendered
                    }
                    Err(e) => {
                        warn!(
                            "单P视频模板渲染失败: '{}', 错误: {}, 使用默认名称: '{}'",
                            config.page_name, e, page.name
                        );
                        page.name.clone()
                    }
                }
            } else {
                // 多P视频使用multi_page_name模板
                match handlebars.render_template(&config.multi_page_name, &page_template_value) {
                    Ok(rendered) => rendered,
                    Err(_) => {
                        // 如果渲染失败，使用默认格式
                        format!("S01E{:02}-{:02}", page.pid, page.pid)
                    }
                }
            };

            let new_page_name = crate::utils::filenamify::filenamify(&rendered_page_name)
                .replace("__SEP__", std::path::MAIN_SEPARATOR_STR);

            // **关键修复：重命名分页的所有相关文件**
            // 从数据库存储的路径或智能查找中获取原始文件名模式（去掉扩展名）
            let old_page_name = if let Some(stored_path) = &page.path {
                let stored_file_path = Path::new(stored_path);
                if let Some(file_stem) = stored_file_path.file_stem() {
                    file_stem.to_string_lossy().to_string()
                } else {
                    // 如果无法从存储路径提取，尝试智能查找
                    find_page_file_pattern(&final_video_path, &page)?
                }
            } else {
                // 数据库中没有存储路径，尝试智能查找
                find_page_file_pattern(&final_video_path, &page)?
            };

            // 如果找到了原始文件名模式，重命名所有相关文件
            if !old_page_name.is_empty() && old_page_name != new_page_name {
                debug!(
                    "准备重命名分页 {} 的文件：{} -> {}",
                    page.pid, old_page_name, new_page_name
                );

                if final_video_path.exists() {
                    if let Ok(entries) = std::fs::read_dir(&final_video_path) {
                        for entry in entries.flatten() {
                            let file_path = entry.path();
                            let file_name = file_path.file_name().unwrap_or_default().to_string_lossy();

                            // 检查文件是否属于当前分页（使用原始文件名模式匹配）
                            // 匹配规则：文件名以原始模式开头，后面可以跟扩展名或其他后缀
                            if file_name.starts_with(&old_page_name) {
                                // 提取原始文件名后面的部分（扩展名和其他后缀）
                                let suffix = file_name.strip_prefix(&old_page_name).unwrap_or("");

                                // 构建新的文件名：新模式 + 原有的后缀
                                let new_file_name = format!("{}{}", new_page_name, suffix);
                                let new_file_path = final_video_path.join(new_file_name);

                                // 只有当新旧路径不同时才进行重命名
                                if file_path != new_file_path {
                                    // **关键修复：检查目标文件是否已存在，避免覆盖**
                                    let final_new_file_path = if new_file_path.exists() {
                                        // 目标文件已存在，生成唯一文件名避免覆盖
                                        let file_stem = new_file_path.file_stem().unwrap_or_default().to_string_lossy();
                                        let file_extension =
                                            new_file_path.extension().unwrap_or_default().to_string_lossy();
                                        let parent_dir = new_file_path.parent().unwrap_or(&final_video_path);

                                        // 尝试添加BV号后缀避免冲突
                                        let bvid_suffix = &video.bvid;
                                        let unique_name = if file_extension.is_empty() {
                                            format!("{}-{}", file_stem, bvid_suffix)
                                        } else {
                                            format!("{}-{}.{}", file_stem, bvid_suffix, file_extension)
                                        };
                                        let unique_path = parent_dir.join(unique_name);

                                        // 如果BV号后缀仍然冲突，使用时间戳
                                        if unique_path.exists() {
                                            let timestamp = chrono::Local::now().format("%H%M%S").to_string();
                                            let final_name = if file_extension.is_empty() {
                                                format!("{}-{}-{}", file_stem, bvid_suffix, timestamp)
                                            } else {
                                                format!(
                                                    "{}-{}-{}.{}",
                                                    file_stem, bvid_suffix, timestamp, file_extension
                                                )
                                            };
                                            parent_dir.join(final_name)
                                        } else {
                                            unique_path
                                        }
                                    } else {
                                        new_file_path.clone()
                                    };

                                    match std::fs::rename(&file_path, &final_new_file_path) {
                                        Ok(_) => {
                                            if final_new_file_path != new_file_path {
                                                warn!(
                                                    "检测到文件名冲突，已重命名避免覆盖: {:?} -> {:?}",
                                                    file_path, final_new_file_path
                                                );
                                            } else {
                                                debug!(
                                                    "重命名分页相关文件成功: {:?} -> {:?}",
                                                    file_path, final_new_file_path
                                                );
                                            }
                                            updated_count += 1;

                                            // 如果这是主文件（MP4），更新数据库中的路径记录
                                            if file_name.ends_with(".mp4") {
                                                let new_path_str = final_new_file_path.to_string_lossy().to_string();
                                                let mut page_update: bili_sync_entity::page::ActiveModel =
                                                    page.clone().into();
                                                page_update.path = Set(Some(new_path_str));
                                                if let Err(e) = page_update.update(db.as_ref()).await {
                                                    warn!("更新数据库中的分页路径失败: {}", e);
                                                } else {
                                                    debug!("更新数据库分页路径成功: {:?}", final_new_file_path);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!(
                                                "重命名分页相关文件失败: {:?} -> {:?}, 错误: {}",
                                                file_path, final_new_file_path, e
                                            );
                                        }
                                    }
                                } else {
                                    debug!("文件路径已经正确，无需重命名: {:?}", file_path);
                                }
                            }
                        }
                    }
                }
            } else {
                debug!(
                    "分页 {} 的文件名已经是正确格式或未找到文件，原始模式: '{}', 新模式: '{}'",
                    page.pid, old_page_name, new_page_name
                );
            }
        }
    }

    info!("文件重命名完成，共处理了 {} 个文件/文件夹", updated_count);
    Ok(updated_count)
}

/// 重新生成已下载视频的NFO文件以使用新的时间类型配置
async fn regenerate_nfo_files(db: Arc<DatabaseConnection>, config: &crate::config::Config) -> Result<u32> {
    use handlebars::Handlebars;
    use sea_orm::*;
    use std::path::Path;

    info!("开始重新生成NFO文件以使用新的时间类型配置...");

    let mut updated_count = 0u32;

    // 获取所有已下载的视频（下载状态大于0表示已下载）
    let videos = bili_sync_entity::video::Entity::find()
        .filter(bili_sync_entity::video::Column::DownloadStatus.gt(0))
        .all(db.as_ref())
        .await?;

    info!("找到 {} 个已下载的视频需要重新生成NFO文件", videos.len());

    for video in videos {
        let video_path = Path::new(&video.path);

        if !video_path.exists() {
            warn!("视频路径不存在，跳过NFO重新生成: {:?}", video_path);
            continue;
        }

        // 获取视频的实际文件名（不包括扩展名）
        let video_base_name = if let Ok(entries) = std::fs::read_dir(video_path) {
            let mut found_name = None;
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.ends_with(".mp4") {
                    // 找到MP4文件，获取其基础名称（不包括.mp4扩展名）
                    found_name = Some(file_name_str.trim_end_matches(".mp4").to_string());
                    break;
                }
            }
            found_name.unwrap_or_else(|| video.name.clone())
        } else {
            video.name.clone()
        };

        // 重新生成视频NFO文件
        let video_nfo_path = if video.single_page.unwrap_or(false) {
            // 单P视频：使用与MP4文件相同的基础名称
            video_path.join(format!("{}.nfo", video_base_name))
        } else {
            // 多P视频：使用视频名称作为NFO文件名
            let video_name = if let Some(video_dir_name) = video_path.file_name() {
                video_dir_name.to_string_lossy().to_string()
            } else {
                crate::utils::filenamify::filenamify(&video.name)
            };
            video_path.join(format!("{}.nfo", video_name))
        };

        let nfo = if video.single_page.unwrap_or(false) {
            NFO::Movie((&video).into())
        } else {
            NFO::TVShow((&video).into())
        };

        match nfo.generate_nfo().await {
            Ok(nfo_content) => {
                if let Some(parent) = video_nfo_path.parent() {
                    if let Err(e) = tokio::fs::create_dir_all(parent).await {
                        warn!("创建NFO文件目录失败: {:?}, 错误: {}", parent, e);
                        continue;
                    }
                }

                match tokio::fs::write(&video_nfo_path, nfo_content.as_bytes()).await {
                    Ok(_) => {
                        debug!("重新生成视频NFO文件成功: {:?}", video_nfo_path);
                        updated_count += 1;
                    }
                    Err(e) => {
                        warn!("写入视频NFO文件失败: {:?}, 错误: {}", video_nfo_path, e);
                    }
                }
            }
            Err(e) => {
                warn!("生成视频NFO内容失败: {}, 错误: {}", video.name, e);
            }
        }

        // 重新生成UP主NFO文件
        let upper_id = video.upper_id.to_string();
        let base_upper_path = &config
            .upper_path
            .join(upper_id.chars().next().unwrap_or('0').to_string())
            .join(&upper_id);
        let upper_nfo_path = base_upper_path.join("person.nfo");
        let upper_nfo = NFO::Upper((&video).into());

        match upper_nfo.generate_nfo().await {
            Ok(nfo_content) => {
                // 确保UP主目录存在
                if let Some(parent) = upper_nfo_path.parent() {
                    if let Err(e) = tokio::fs::create_dir_all(parent).await {
                        warn!("创建UP主NFO文件目录失败: {:?}, 错误: {}", parent, e);
                        continue;
                    }
                }

                match tokio::fs::write(&upper_nfo_path, nfo_content.as_bytes()).await {
                    Ok(_) => {
                        debug!("重新生成UP主NFO文件成功: {:?}", upper_nfo_path);
                        updated_count += 1;
                    }
                    Err(e) => {
                        warn!("写入UP主NFO文件失败: {:?}, 错误: {}", upper_nfo_path, e);
                    }
                }
            }
            Err(e) => {
                warn!("生成UP主NFO内容失败: {}, 错误: {}", video.name, e);
            }
        }

        // 重新生成分页NFO文件（对多P视频或番剧类型）
        let is_bangumi = video.source_type == Some(1);
        if !video.single_page.unwrap_or(false) || is_bangumi {
            if is_bangumi {
                // 对于番剧，需要获取番剧源的模板配置
                let bangumi_source = bili_sync_entity::video_source::Entity::find()
                    .filter(bili_sync_entity::video_source::Column::Id.eq(video.source_id.unwrap_or(0)))
                    .filter(bili_sync_entity::video_source::Column::Type.eq(1))
                    .one(db.as_ref())
                    .await?;

                let pages = bili_sync_entity::page::Entity::find()
                    .filter(bili_sync_entity::page::Column::VideoId.eq(video.id))
                    .filter(bili_sync_entity::page::Column::DownloadStatus.gt(0))
                    .all(db.as_ref())
                    .await?;

                for page in pages {
                    // 对于番剧，首先尝试从现有文件中查找实际的文件名格式
                    let actual_page_name = if let Ok(entries) = std::fs::read_dir(video_path) {
                        let mut found_name = None;
                        let episode_number = video.episode_number.unwrap_or(page.pid);

                        for entry in entries.flatten() {
                            let file_name = entry.file_name();
                            let file_name_str = file_name.to_string_lossy();
                            if file_name_str.ends_with(".mp4") {
                                // 尝试多种匹配模式
                                let patterns = [
                                    format!("第{:02}集", episode_number),       // 第01集
                                    format!("第{}集", episode_number),          // 第1集
                                    format!("S{:02}E{:02}", 1, episode_number), // S01E01
                                    format!("E{:02}", episode_number),          // E01
                                    format!("{:02}", episode_number),           // 01
                                ];

                                if patterns.iter().any(|pattern| file_name_str.contains(pattern)) {
                                    // 找到匹配的视频文件，提取基础名称
                                    found_name = Some(file_name_str.trim_end_matches(".mp4").to_string());
                                    debug!("找到番剧分页文件: {} 对应集数: {}", file_name_str, episode_number);
                                    break;
                                }
                            }
                        }
                        found_name
                    } else {
                        None
                    };

                    let page_name = if let Some(actual_name) = actual_page_name {
                        // 使用实际找到的文件名格式
                        actual_name
                    } else {
                        // 如果找不到实际文件，则使用模板生成（兜底方案）
                        if let Some(bangumi_source) = &bangumi_source {
                            let template = bangumi_source
                                .page_name_template
                                .as_deref()
                                .unwrap_or("S{{season_pad}}E{{pid_pad}}-{{pid_pad}}");

                            // 构建番剧专用的模板数据
                            let episode_number = video.episode_number.unwrap_or(page.pid);
                            let season_number = video.season_number.unwrap_or(1);

                            let mut template_data = std::collections::HashMap::new();
                            template_data.insert("bvid".to_string(), serde_json::Value::String(video.bvid.clone()));
                            template_data.insert("title".to_string(), serde_json::Value::String(video.name.clone()));
                            template_data.insert(
                                "upper_name".to_string(),
                                serde_json::Value::String(video.upper_name.clone()),
                            );
                            template_data.insert(
                                "upper_mid".to_string(),
                                serde_json::Value::String(video.upper_id.to_string()),
                            );
                            template_data.insert("ptitle".to_string(), serde_json::Value::String(page.name.clone()));
                            template_data
                                .insert("pid".to_string(), serde_json::Value::String(episode_number.to_string()));
                            template_data.insert(
                                "pid_pad".to_string(),
                                serde_json::Value::String(format!("{:02}", episode_number)),
                            );
                            template_data.insert(
                                "season".to_string(),
                                serde_json::Value::String(season_number.to_string()),
                            );
                            template_data.insert(
                                "season_pad".to_string(),
                                serde_json::Value::String(format!("{:02}", season_number)),
                            );

                            let handlebars = Handlebars::new();
                            let template_value = serde_json::Value::Object(template_data.into_iter().collect());

                            match handlebars.render_template(template, &template_value) {
                                Ok(rendered) => crate::utils::filenamify::filenamify(&rendered),
                                Err(e) => {
                                    warn!("渲染番剧模板失败: {}, 使用默认格式", e);
                                    format!("S{:02}E{:02}-{:02}", season_number, episode_number, episode_number)
                                }
                            }
                        } else {
                            // 如果找不到番剧源配置，使用默认格式
                            let season_number = video.season_number.unwrap_or(1);
                            let episode_number = video.episode_number.unwrap_or(page.pid);
                            format!("S{:02}E{:02}-{:02}", season_number, episode_number, episode_number)
                        }
                    };

                    let page_nfo_path = video_path.join(format!("{}.nfo", page_name));
                    let page_nfo = NFO::Episode((&page).into());

                    match page_nfo.generate_nfo().await {
                        Ok(nfo_content) => match tokio::fs::write(&page_nfo_path, nfo_content.as_bytes()).await {
                            Ok(_) => {
                                debug!("重新生成番剧分页NFO文件成功: {:?}", page_nfo_path);
                                updated_count += 1;
                            }
                            Err(e) => {
                                warn!("写入番剧分页NFO文件失败: {:?}, 错误: {}", page_nfo_path, e);
                            }
                        },
                        Err(e) => {
                            warn!("生成分页NFO内容失败: {}", e);
                        }
                    }
                }
            } else {
                // 对于普通多P视频，需要使用与实际文件相同的命名模式
                let pages = bili_sync_entity::page::Entity::find()
                    .filter(bili_sync_entity::page::Column::VideoId.eq(video.id))
                    .filter(bili_sync_entity::page::Column::DownloadStatus.gt(0))
                    .all(db.as_ref())
                    .await?;

                for page in pages {
                    // 使用multi_page_name模板来生成正确的文件名
                    let mut template_data = std::collections::HashMap::new();
                    template_data.insert("title".to_string(), serde_json::Value::String(video.name.clone()));
                    template_data.insert("show_title".to_string(), serde_json::Value::String(video.name.clone()));
                    template_data.insert("bvid".to_string(), serde_json::Value::String(video.bvid.clone()));
                    template_data.insert(
                        "upper_name".to_string(),
                        serde_json::Value::String(video.upper_name.clone()),
                    );
                    template_data.insert(
                        "upper_mid".to_string(),
                        serde_json::Value::String(video.upper_id.to_string()),
                    );
                    template_data.insert("ptitle".to_string(), serde_json::Value::String(page.name.clone()));
                    template_data.insert("pid".to_string(), serde_json::Value::String(page.pid.to_string()));
                    template_data.insert(
                        "pid_pad".to_string(),
                        serde_json::Value::String(format!("{:02}", page.pid)),
                    );
                    template_data.insert("season".to_string(), serde_json::Value::String("1".to_string()));
                    template_data.insert("season_pad".to_string(), serde_json::Value::String("01".to_string()));
                    template_data.insert(
                        "duration".to_string(),
                        serde_json::Value::String(page.duration.to_string()),
                    );

                    if let Some(width) = page.width {
                        template_data.insert("width".to_string(), serde_json::Value::String(width.to_string()));
                    }

                    if let Some(height) = page.height {
                        template_data.insert("height".to_string(), serde_json::Value::String(height.to_string()));
                    }

                    // 格式化时间
                    let formatted_pubtime = video.pubtime.format(&config.time_format).to_string();
                    template_data.insert("pubtime".to_string(), serde_json::Value::String(formatted_pubtime));

                    let formatted_favtime = video.favtime.format(&config.time_format).to_string();
                    template_data.insert("fav_time".to_string(), serde_json::Value::String(formatted_favtime));

                    let formatted_ctime = video.ctime.format(&config.time_format).to_string();
                    template_data.insert("ctime".to_string(), serde_json::Value::String(formatted_ctime));

                    // 使用multi_page_name模板渲染文件名
                    let handlebars = Handlebars::new();
                    let template_value = serde_json::Value::Object(template_data.into_iter().collect());

                    let page_name = match handlebars.render_template(&config.multi_page_name, &template_value) {
                        Ok(rendered) => crate::utils::filenamify::filenamify(&rendered),
                        Err(e) => {
                            warn!("渲染多P视频模板失败: {}, 使用默认格式", e);
                            format!("S01E{:02}-{:02}", page.pid, page.pid)
                        }
                    };

                    let page_nfo_path = video_path.join(format!("{}.nfo", page_name));
                    let page_nfo = NFO::Episode((&page).into());

                    match page_nfo.generate_nfo().await {
                        Ok(nfo_content) => match tokio::fs::write(&page_nfo_path, nfo_content.as_bytes()).await {
                            Ok(_) => {
                                debug!("重新生成分页NFO文件成功: {:?}", page_nfo_path);
                                updated_count += 1;
                            }
                            Err(e) => {
                                warn!("写入分页NFO文件失败: {:?}, 错误: {}", page_nfo_path, e);
                            }
                        },
                        Err(e) => {
                            warn!("生成分页NFO内容失败: {}", e);
                        }
                    }
                }
            }
        }
    }

    info!("NFO文件重新生成完成，共重新生成了 {} 个NFO文件", updated_count);
    Ok(updated_count)
}

/// 获取番剧的所有季度信息
#[utoipa::path(
    get,
    path = "/api/bangumi/seasons/{season_id}",
    responses(
        (status = 200, body = ApiResponse<Vec<BangumiSeasonInfo>>),
    )
)]
pub async fn get_bangumi_seasons(
    Path(season_id): Path<String>,
) -> Result<ApiResponse<crate::api::response::BangumiSeasonsResponse>, ApiError> {
    use crate::bilibili::bangumi::Bangumi;
    use crate::bilibili::BiliClient;
    use futures::future::join_all;

    // 创建 BiliClient，使用空 cookie（对于获取季度信息不需要登录）
    let bili_client = BiliClient::new(String::new());

    // 创建 Bangumi 实例
    let bangumi = Bangumi::new(&bili_client, None, Some(season_id.clone()), None);

    // 获取所有季度信息
    match bangumi.get_all_seasons().await {
        Ok(seasons) => {
            // 并发获取所有季度的详细信息
            let season_details_futures: Vec<_> = seasons
                .iter()
                .map(|s| {
                    let bili_client_clone = bili_client.clone();
                    let season_clone = s.clone();
                    async move {
                        let season_bangumi = Bangumi::new(
                            &bili_client_clone,
                            season_clone.media_id.clone(),
                            Some(season_clone.season_id.clone()),
                            None,
                        );

                        let (full_title, episode_count, description) = match season_bangumi.get_season_info().await {
                            Ok(season_info) => {
                                let full_title = season_info["title"].as_str().map(|t| t.to_string());

                                // 获取集数信息
                                let episode_count =
                                    season_info["episodes"].as_array().map(|episodes| episodes.len() as i32);

                                // 获取简介信息
                                let description = season_info["evaluate"].as_str().map(|d| d.to_string());

                                (full_title, episode_count, description)
                            }
                            Err(e) => {
                                warn!("获取季度 {} 的详细信息失败: {}", season_clone.season_id, e);
                                (None, None, None)
                            }
                        };

                        (season_clone, full_title, episode_count, description)
                    }
                })
                .collect();

            // 等待所有并发请求完成
            let season_details = join_all(season_details_futures).await;

            // 构建响应数据
            let season_list: Vec<_> = season_details
                .into_iter()
                .map(
                    |(s, full_title, episode_count, description)| crate::api::response::BangumiSeasonInfo {
                        season_id: s.season_id,
                        season_title: s.season_title,
                        full_title,
                        media_id: s.media_id,
                        cover: Some(s.cover),
                        episode_count,
                        description,
                    },
                )
                .collect();

            Ok(ApiResponse::ok(crate::api::response::BangumiSeasonsResponse {
                success: true,
                data: season_list,
            }))
        }
        Err(e) => {
            error!("获取番剧季度信息失败: {}", e);
            Err(anyhow!("获取番剧季度信息失败: {}", e).into())
        }
    }
}

/// 搜索bilibili内容
#[utoipa::path(
    get,
    path = "/api/search",
    params(
        ("keyword" = String, Query, description = "搜索关键词"),
        ("search_type" = String, Query, description = "搜索类型：video, bili_user, media_bangumi"),
        ("page" = Option<u32>, Query, description = "页码，默认1"),
        ("page_size" = Option<u32>, Query, description = "每页数量，默认20")
    ),
    responses(
        (status = 200, body = ApiResponse<crate::api::response::SearchResponse>),
    )
)]
pub async fn search_bilibili(
    Query(params): Query<crate::api::request::SearchRequest>,
) -> Result<ApiResponse<crate::api::response::SearchResponse>, ApiError> {
    use crate::bilibili::{BiliClient, SearchResult};

    // 验证搜索类型
    let valid_types = ["video", "bili_user", "media_bangumi", "media_ft"];
    if !valid_types.contains(&params.search_type.as_str()) {
        return Err(anyhow!("不支持的搜索类型，支持的类型: {}", valid_types.join(", ")).into());
    }

    // 验证关键词
    if params.keyword.trim().is_empty() {
        return Err(anyhow!("搜索关键词不能为空").into());
    }

    // 创建 BiliClient，使用空 cookie（搜索不需要登录）
    let bili_client = BiliClient::new(String::new());

    // 特殊处理：当搜索类型为media_bangumi时，同时搜索番剧和影视
    let mut all_results = Vec::new();
    let mut total_results = 0u32;

    if params.search_type == "media_bangumi" {
        // 搜索番剧
        match bili_client
            .search(
                &params.keyword,
                "media_bangumi",
                params.page,
                params.page_size / 2, // 每种类型分配一半的结果数
            )
            .await
        {
            Ok(bangumi_wrapper) => {
                all_results.extend(bangumi_wrapper.results);
                total_results += bangumi_wrapper.total;
            }
            Err(e) => {
                warn!("搜索番剧失败: {}", e);
            }
        }

        // 搜索影视
        match bili_client
            .search(
                &params.keyword,
                "media_ft",
                params.page,
                params.page_size / 2, // 每种类型分配一半的结果数
            )
            .await
        {
            Ok(ft_wrapper) => {
                all_results.extend(ft_wrapper.results);
                total_results += ft_wrapper.total;
            }
            Err(e) => {
                warn!("搜索影视失败: {}", e);
            }
        }

        // 如果两个搜索都失败了，返回错误
        if all_results.is_empty() && total_results == 0 {
            return Err(anyhow!("搜索失败：无法获取番剧或影视结果").into());
        }
    } else {
        // 其他类型正常搜索
        match bili_client
            .search(&params.keyword, &params.search_type, params.page, params.page_size)
            .await
        {
            Ok(search_wrapper) => {
                all_results = search_wrapper.results;
                total_results = search_wrapper.total;
            }
            Err(e) => {
                error!("搜索失败: {}", e);
                return Err(anyhow!("搜索失败: {}", e).into());
            }
        }
    }

    // 转换搜索结果格式
    let api_results: Vec<crate::api::response::SearchResult> = all_results
        .into_iter()
        .map(|r: SearchResult| crate::api::response::SearchResult {
            result_type: r.result_type,
            title: r.title,
            author: r.author,
            bvid: r.bvid,
            aid: r.aid,
            mid: r.mid,
            season_id: r.season_id,
            media_id: r.media_id,
            cover: r.cover,
            description: r.description,
            duration: r.duration,
            pubdate: r.pubdate,
            play: r.play,
            danmaku: r.danmaku,
        })
        .collect();

    Ok(ApiResponse::ok(crate::api::response::SearchResponse {
        success: true,
        results: api_results,
        total: total_results,
        page: params.page,
        page_size: params.page_size,
    }))
}

/// 获取用户收藏夹列表
#[utoipa::path(
    get,
    path = "/api/user/favorites",
    responses(
        (status = 200, body = ApiResponse<Vec<crate::api::response::UserFavoriteFolder>>),
    )
)]
pub async fn get_user_favorites() -> Result<ApiResponse<Vec<crate::api::response::UserFavoriteFolder>>, ApiError> {
    let bili_client = crate::bilibili::BiliClient::new(String::new());

    match bili_client.get_user_favorite_folders(None).await {
        Ok(folders) => {
            let response_folders: Vec<crate::api::response::UserFavoriteFolder> = folders
                .into_iter()
                .map(|folder| crate::api::response::UserFavoriteFolder {
                    id: folder.id,
                    fid: folder.fid,
                    title: folder.title,
                    media_count: folder.media_count,
                })
                .collect();

            Ok(ApiResponse::ok(response_folders))
        }
        Err(e) => {
            error!("获取用户收藏夹列表失败: {}", e);
            Err(anyhow!("获取用户收藏夹列表失败: {}", e).into())
        }
    }
}

/// 获取UP主的合集和系列列表
#[utoipa::path(
    get,
    path = "/api/user/collections/{mid}",
    params(
        ("mid" = i64, Path, description = "UP主ID"),
        ("page" = Option<u32>, Query, description = "页码，默认1"),
        ("page_size" = Option<u32>, Query, description = "每页数量，默认20")
    ),
    responses(
        (status = 200, body = ApiResponse<crate::api::response::UserCollectionsResponse>),
    )
)]
pub async fn get_user_collections(
    Path(mid): Path<i64>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<ApiResponse<crate::api::response::UserCollectionsResponse>, ApiError> {
    let page = params.get("page").and_then(|p| p.parse::<u32>().ok()).unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(20);

    let bili_client = crate::bilibili::BiliClient::new(String::new());

    match bili_client.get_user_collections(mid, page, page_size).await {
        Ok(response) => Ok(ApiResponse::ok(response)),
        Err(e) => {
            let error_msg = format!("获取UP主 {} 的合集失败", mid);
            warn!("{}: {}", error_msg, e);

            // 检查是否是网络错误，提供更友好的错误信息
            let user_friendly_error =
                if e.to_string().contains("ERR_EMPTY_RESPONSE") || e.to_string().contains("Failed to fetch") {
                    "该UP主的合集可能需要登录访问，或暂时无法获取。请稍后重试或手动输入合集ID。".to_string()
                } else if e.to_string().contains("403") || e.to_string().contains("Forbidden") {
                    "该UP主的合集为私有，无法访问。".to_string()
                } else if e.to_string().contains("404") || e.to_string().contains("Not Found") {
                    "UP主不存在或合集已被删除。".to_string()
                } else {
                    "网络错误或服务暂时不可用，请稍后重试。".to_string()
                };

            Err(anyhow!("{}", user_friendly_error).into())
        }
    }
}

/// 计算目录大小的辅助函数
fn get_directory_size(path: &str) -> std::io::Result<u64> {
    fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
        let mut size = 0;
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    size += dir_size(&path)?;
                } else {
                    size += entry.metadata()?.len();
                }
            }
        }
        Ok(size)
    }

    let path = std::path::Path::new(path);
    dir_size(path)
}

/// 获取关注的UP主列表
#[utoipa::path(
    get,
    path = "/api/user/followings",
    responses(
        (status = 200, body = ApiResponse<Vec<crate::api::response::UserFollowing>>),
    )
)]
pub async fn get_user_followings() -> Result<ApiResponse<Vec<crate::api::response::UserFollowing>>, ApiError> {
    let bili_client = crate::bilibili::BiliClient::new(String::new());

    match bili_client.get_user_followings().await {
        Ok(followings) => {
            let response_followings: Vec<crate::api::response::UserFollowing> = followings
                .into_iter()
                .map(|following| crate::api::response::UserFollowing {
                    mid: following.mid,
                    name: following.name,
                    face: following.face,
                    sign: following.sign,
                    official_verify: following
                        .official_verify
                        .map(|verify| crate::api::response::OfficialVerify {
                            type_: verify.type_,
                            desc: verify.desc,
                        }),
                })
                .collect();
            Ok(ApiResponse::ok(response_followings))
        }
        Err(e) => {
            tracing::error!("获取关注UP主列表失败: {}", e);
            Err(ApiError::from(anyhow::anyhow!("获取关注UP主列表失败: {}", e)))
        }
    }
}

/// 获取订阅的合集列表
#[utoipa::path(
    get,
    path = "/api/user/subscribed-collections",
    responses(
        (status = 200, body = ApiResponse<Vec<crate::api::response::UserCollectionInfo>>),
    )
)]
pub async fn get_subscribed_collections() -> Result<ApiResponse<Vec<crate::api::response::UserCollectionInfo>>, ApiError>
{
    let bili_client = crate::bilibili::BiliClient::new(String::new());

    match bili_client.get_subscribed_collections().await {
        Ok(collections) => Ok(ApiResponse::ok(collections)),
        Err(e) => {
            tracing::error!("获取订阅合集失败: {}", e);
            Err(ApiError::from(anyhow::anyhow!("获取订阅合集失败: {}", e)))
        }
    }
}

/// 日志级别枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub enum LogLevel {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warn")]
    Warn,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "debug")]
    Debug,
}

/// 日志条目结构
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
    pub target: Option<String>,
}

/// 日志响应结构
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LogsResponse {
    pub logs: Vec<LogEntry>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
}

// 全局日志存储，使用Arc<Mutex<VecDeque<LogEntry>>>来存储最近的日志
lazy_static::lazy_static! {
    static ref LOG_BUFFER: Arc<Mutex<VecDeque<LogEntry>>> = Arc::new(Mutex::new(VecDeque::with_capacity(100000)));
    // 为debug日志单独设置缓冲区，容量较小
    static ref DEBUG_LOG_BUFFER: Arc<Mutex<VecDeque<LogEntry>>> = Arc::new(Mutex::new(VecDeque::with_capacity(10000)));
    static ref LOG_BROADCASTER: broadcast::Sender<LogEntry> = {
        let (sender, _) = broadcast::channel(100);
        sender
    };
}

/// 添加日志到缓冲区
pub fn add_log_entry(level: LogLevel, message: String, target: Option<String>) {
    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        level: level.clone(), // 克隆level避免所有权问题
        message,
        target,
    };

    match level {
        LogLevel::Debug => {
            // Debug日志使用单独的缓冲区，容量较小
            if let Ok(mut buffer) = DEBUG_LOG_BUFFER.lock() {
                buffer.push_back(entry.clone());
                // Debug日志保持在10000条以内
                if buffer.len() > 10000 {
                    buffer.pop_front();
                }
            }
        }
        _ => {
            // 其他级别日志使用主缓冲区
            if let Ok(mut buffer) = LOG_BUFFER.lock() {
                buffer.push_back(entry.clone());
                // 主缓冲区保持在50000条以内（给debug留出空间）
                if buffer.len() > 50000 {
                    buffer.pop_front();
                }
            }
        }
    }

    // 广播给实时订阅者
    let _ = LOG_BROADCASTER.send(entry);
}

/// 获取历史日志
#[utoipa::path(
    get,
    path = "/api/logs",
    params(
        ("level" = Option<String>, Query, description = "过滤日志级别: info, warn, error, debug"),
        ("limit" = Option<usize>, Query, description = "每页返回的日志数量，默认100，最大10000"),
        ("page" = Option<usize>, Query, description = "页码，从1开始，默认1")
    ),
    responses(
        (status = 200, description = "获取日志成功", body = LogsResponse),
        (status = 500, description = "服务器内部错误", body = String)
    )
)]
pub async fn get_logs(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<ApiResponse<LogsResponse>, ApiError> {
    let level_filter = params.get("level").and_then(|l| match l.as_str() {
        "info" => Some(LogLevel::Info),
        "warn" => Some(LogLevel::Warn),
        "error" => Some(LogLevel::Error),
        "debug" => Some(LogLevel::Debug),
        _ => None,
    });

    let limit = params
        .get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(100)
        .min(10000); // 提高最大限制到10000条

    let page = params
        .get("page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1); // 页码最小为1

    let logs = if let Some(ref filter_level) = level_filter {
        if *filter_level == LogLevel::Debug {
            // 如果筛选debug级别，从debug专用缓冲区获取
            if let Ok(buffer) = DEBUG_LOG_BUFFER.lock() {
                let total_logs: Vec<LogEntry> = buffer
                    .iter()
                    .rev() // 最新的在前
                    .cloned()
                    .collect();

                let total = total_logs.len();
                let offset = (page - 1) * limit;
                let total_pages = if total == 0 { 0 } else { total.div_ceil(limit) };
                let logs = total_logs.into_iter().skip(offset).take(limit).collect();

                LogsResponse {
                    logs,
                    total,
                    page,
                    per_page: limit,
                    total_pages,
                }
            } else {
                LogsResponse {
                    logs: vec![],
                    total: 0,
                    page: 1,
                    per_page: limit,
                    total_pages: 0,
                }
            }
        } else {
            // 其他级别从主缓冲区获取
            if let Ok(buffer) = LOG_BUFFER.lock() {
                let total_logs: Vec<LogEntry> = buffer
                    .iter()
                    .rev() // 最新的在前
                    .filter(|entry| &entry.level == filter_level)
                    .cloned()
                    .collect();

                let total = total_logs.len();
                let offset = (page - 1) * limit;
                let total_pages = if total == 0 { 0 } else { total.div_ceil(limit) };
                let logs = total_logs.into_iter().skip(offset).take(limit).collect();

                LogsResponse {
                    logs,
                    total,
                    page,
                    per_page: limit,
                    total_pages,
                }
            } else {
                LogsResponse {
                    logs: vec![],
                    total: 0,
                    page: 1,
                    per_page: limit,
                    total_pages: 0,
                }
            }
        }
    } else {
        // 没有指定级别（全部日志），合并两个缓冲区但排除debug级别
        if let Ok(main_buffer) = LOG_BUFFER.lock() {
            let total_logs: Vec<LogEntry> = main_buffer
                .iter()
                .rev() // 最新的在前
                .filter(|entry| entry.level != LogLevel::Debug) // 排除debug级别
                .cloned()
                .collect();

            let total = total_logs.len();
            let offset = (page - 1) * limit;
            let total_pages = if total == 0 { 0 } else { total.div_ceil(limit) };
            let logs = total_logs.into_iter().skip(offset).take(limit).collect();

            LogsResponse {
                logs,
                total,
                page,
                per_page: limit,
                total_pages,
            }
        } else {
            LogsResponse {
                logs: vec![],
                total: 0,
                page: 1,
                per_page: limit,
                total_pages: 0,
            }
        }
    };

    Ok(ApiResponse::ok(logs))
}

/// 队列任务信息结构体
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QueueTaskInfo {
    pub task_id: String,
    pub task_type: String,
    pub description: String,
    pub created_at: String,
}

/// 队列状态响应结构体
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QueueStatusResponse {
    pub is_scanning: bool,
    pub delete_queue: QueueInfo,
    pub video_delete_queue: QueueInfo,
    pub add_queue: QueueInfo,
    pub config_queue: ConfigQueueInfo,
}

/// 队列信息结构体
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QueueInfo {
    pub length: usize,
    pub is_processing: bool,
    pub tasks: Vec<QueueTaskInfo>,
}

/// 配置队列信息结构体
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConfigQueueInfo {
    pub update_length: usize,
    pub reload_length: usize,
    pub is_processing: bool,
    pub update_tasks: Vec<QueueTaskInfo>,
    pub reload_tasks: Vec<QueueTaskInfo>,
}

/// 获取队列状态
#[utoipa::path(
    get,
    path = "/api/queue/status",
    responses(
        (status = 200, description = "获取队列状态成功", body = QueueStatusResponse),
        (status = 500, description = "服务器内部错误", body = String)
    )
)]
pub async fn get_queue_status() -> Result<ApiResponse<QueueStatusResponse>, ApiError> {
    use crate::task::{ADD_TASK_QUEUE, CONFIG_TASK_QUEUE, DELETE_TASK_QUEUE, TASK_CONTROLLER, VIDEO_DELETE_TASK_QUEUE};

    // 获取扫描状态
    let is_scanning = TASK_CONTROLLER.is_scanning();

    // 获取删除队列状态
    let delete_queue_length = DELETE_TASK_QUEUE.queue_length().await;
    let delete_is_processing = DELETE_TASK_QUEUE.is_processing();

    // 这里只获取队列长度，不获取具体任务内容以保护敏感信息
    let delete_tasks = (0..delete_queue_length)
        .map(|i| QueueTaskInfo {
            task_id: format!("delete_{}", i + 1),
            task_type: "delete_video_source".to_string(),
            description: "删除视频源任务".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .collect();

    // 获取视频删除队列状态
    let video_delete_queue_length = VIDEO_DELETE_TASK_QUEUE.queue_length().await;
    let video_delete_is_processing = VIDEO_DELETE_TASK_QUEUE.is_processing();

    let video_delete_tasks = (0..video_delete_queue_length)
        .map(|i| QueueTaskInfo {
            task_id: format!("video_delete_{}", i + 1),
            task_type: "delete_video".to_string(),
            description: "删除视频任务".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .collect();

    // 获取添加队列状态
    let add_queue_length = ADD_TASK_QUEUE.queue_length().await;
    let add_is_processing = ADD_TASK_QUEUE.is_processing();

    let add_tasks = (0..add_queue_length)
        .map(|i| QueueTaskInfo {
            task_id: format!("add_{}", i + 1),
            task_type: "add_video_source".to_string(),
            description: "添加视频源任务".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .collect();

    // 获取配置队列状态
    let config_update_length = CONFIG_TASK_QUEUE.update_queue_length().await;
    let config_reload_length = CONFIG_TASK_QUEUE.reload_queue_length().await;
    let config_is_processing = CONFIG_TASK_QUEUE.is_processing();

    let config_update_tasks = (0..config_update_length)
        .map(|i| QueueTaskInfo {
            task_id: format!("config_update_{}", i + 1),
            task_type: "update_config".to_string(),
            description: "更新配置任务".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .collect();

    let config_reload_tasks = (0..config_reload_length)
        .map(|i| QueueTaskInfo {
            task_id: format!("config_reload_{}", i + 1),
            task_type: "reload_config".to_string(),
            description: "重载配置任务".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .collect();

    let response = QueueStatusResponse {
        is_scanning,
        delete_queue: QueueInfo {
            length: delete_queue_length,
            is_processing: delete_is_processing,
            tasks: delete_tasks,
        },
        video_delete_queue: QueueInfo {
            length: video_delete_queue_length,
            is_processing: video_delete_is_processing,
            tasks: video_delete_tasks,
        },
        add_queue: QueueInfo {
            length: add_queue_length,
            is_processing: add_is_processing,
            tasks: add_tasks,
        },
        config_queue: ConfigQueueInfo {
            update_length: config_update_length,
            reload_length: config_reload_length,
            is_processing: config_is_processing,
            update_tasks: config_update_tasks,
            reload_tasks: config_reload_tasks,
        },
    };

    Ok(ApiResponse::ok(response))
}

/// 代理B站图片请求，解决防盗链问题
#[utoipa::path(
    get,
    path = "/api/proxy/image",
    params(
        ("url" = String, Query, description = "图片URL"),
    ),
    responses(
        (status = 200, description = "图片数据", content_type = "image/*"),
        (status = 400, description = "无效的URL"),
        (status = 404, description = "图片不存在"),
    )
)]
pub async fn proxy_image(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    let url = params.get("url").ok_or_else(|| anyhow!("缺少url参数"))?;

    // 验证URL是否来自B站
    if !url.contains("hdslb.com") && !url.contains("bilibili.com") {
        return Err(anyhow!("只支持B站图片URL").into());
    }

    // 创建HTTP客户端
    let client = reqwest::Client::new();

    // 请求图片，添加必要的请求头
    let response = client
        .get(url)
        .header("Referer", "https://www.bilibili.com/")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .send()
        .await
        .map_err(|e| anyhow!("请求图片失败: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow!("图片请求失败: {}", response.status()).into());
    }

    // 获取内容类型
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();

    // 获取图片数据
    let image_data = response.bytes().await.map_err(|e| anyhow!("读取图片数据失败: {}", e))?;

    // 返回图片响应
    Ok(axum::response::Response::builder()
        .status(200)
        .header("Content-Type", content_type.as_str())
        .header("Cache-Control", "public, max-age=3600") // 缓存1小时
        .body(axum::body::Body::from(image_data))
        .unwrap())
}

/// 同步数据库中的时间戳到新时区
///
/// 该函数会将数据库中所有的时间戳字段从旧时区转换为新时区
/// 包括：视频源表的created_at、视频表的时间戳字段、页面表的created_at等
async fn sync_database_timestamps(db: Arc<DatabaseConnection>, old_timezone: &str, new_timezone: &str) -> Result<u32> {
    use sea_orm::ConnectionTrait;

    let mut updated_count = 0u32;

    // 解析时区
    let old_tz = match old_timezone {
        "Asia/Shanghai" => 8,
        "UTC" => 0,
        "America/New_York" => -5,
        "America/Los_Angeles" => -8,
        "Europe/London" => 0,
        "Europe/Paris" => 1,
        "Asia/Tokyo" => 9,
        "Asia/Seoul" => 9,
        "Australia/Sydney" => 10,
        "Asia/Dubai" => 4,
        "Asia/Singapore" => 8,
        "Asia/Hong_Kong" => 8,
        "Asia/Taipei" => 8,
        _ => 8, // 默认北京时间
    };

    let new_tz = match new_timezone {
        "Asia/Shanghai" => 8,
        "UTC" => 0,
        "America/New_York" => -5,
        "America/Los_Angeles" => -8,
        "Europe/London" => 0,
        "Europe/Paris" => 1,
        "Asia/Tokyo" => 9,
        "Asia/Seoul" => 9,
        "Australia/Sydney" => 10,
        "Asia/Dubai" => 4,
        "Asia/Singapore" => 8,
        "Asia/Hong_Kong" => 8,
        "Asia/Taipei" => 8,
        _ => 8, // 默认北京时间
    };

    let offset_hours = new_tz - old_tz;

    if offset_hours == 0 {
        info!("时区偏移为0，无需同步数据库时间戳");
        return Ok(0);
    }

    info!("开始同步数据库时间戳，时区偏移: {} 小时", offset_hours);

    // 构建时间偏移的SQL表达式
    let offset_sql = if offset_hours > 0 {
        format!("datetime({{field}}, '+{} hours')", offset_hours)
    } else {
        format!("datetime({{field}}, '{} hours')", offset_hours)
    };

    // 更新视频源表的 created_at 字段
    let tables_with_created_at = vec!["favorite", "collection", "watch_later", "submission"];

    for table in tables_with_created_at {
        // 先处理标准格式的时间戳
        let sql = format!(
            "UPDATE {} SET created_at = {} WHERE created_at IS NOT NULL AND created_at != '' AND datetime(created_at) IS NOT NULL",
            table,
            offset_sql.replace("{field}", "created_at")
        );

        match db.execute_unprepared(&sql).await {
            Ok(result) => {
                let rows_affected = result.rows_affected();
                updated_count += rows_affected as u32;
                debug!(
                    "更新表 {} 的 created_at 字段（标准格式），影响 {} 行",
                    table, rows_affected
                );
            }
            Err(e) => {
                error!("更新表 {} 的 created_at 字段（标准格式）失败: {}", table, e);
            }
        }

        // 处理带UTC后缀的时间戳
        let utc_sql = format!(
            "UPDATE {} SET created_at = {} WHERE created_at IS NOT NULL AND created_at LIKE '% UTC' AND datetime(REPLACE(created_at, ' UTC', '')) IS NOT NULL",
            table,
            offset_sql.replace("{field}", "REPLACE(created_at, ' UTC', '')")
        );

        match db.execute_unprepared(&utc_sql).await {
            Ok(result) => {
                let rows_affected = result.rows_affected();
                updated_count += rows_affected as u32;
                debug!(
                    "更新表 {} 的 created_at 字段（UTC格式），影响 {} 行",
                    table, rows_affected
                );
            }
            Err(e) => {
                error!("更新表 {} 的 created_at 字段（UTC格式）失败: {}", table, e);
            }
        }
    }

    // 更新视频表的时间戳字段
    let video_timestamp_fields = vec!["ctime", "pubtime", "favtime", "created_at"];

    for field in video_timestamp_fields {
        // 先处理标准格式的时间戳
        let sql = if field == "created_at" {
            format!(
                "UPDATE video SET {} = {} WHERE {} IS NOT NULL AND {} != '' AND datetime({}) IS NOT NULL",
                field,
                offset_sql.replace("{field}", field),
                field,
                field,
                field
            )
        } else {
            format!(
                "UPDATE video SET {} = {} WHERE {} IS NOT NULL AND datetime({}) IS NOT NULL",
                field,
                offset_sql.replace("{field}", field),
                field,
                field
            )
        };

        match db.execute_unprepared(&sql).await {
            Ok(result) => {
                let rows_affected = result.rows_affected();
                updated_count += rows_affected as u32;
                debug!("更新视频表的 {} 字段（标准格式），影响 {} 行", field, rows_affected);
            }
            Err(e) => {
                error!("更新视频表的 {} 字段（标准格式）失败: {}", field, e);
            }
        }

        // 处理带UTC后缀的时间戳（主要针对created_at字段）
        if field == "created_at" {
            let utc_sql = format!(
                "UPDATE video SET {} = {} WHERE {} IS NOT NULL AND {} LIKE '% UTC' AND datetime(REPLACE({}, ' UTC', '')) IS NOT NULL",
                field,
                offset_sql.replace("{field}", &format!("REPLACE({}, ' UTC', '')", field)),
                field,
                field,
                field
            );

            match db.execute_unprepared(&utc_sql).await {
                Ok(result) => {
                    let rows_affected = result.rows_affected();
                    updated_count += rows_affected as u32;
                    debug!("更新视频表的 {} 字段（UTC格式），影响 {} 行", field, rows_affected);
                }
                Err(e) => {
                    error!("更新视频表的 {} 字段（UTC格式）失败: {}", field, e);
                }
            }
        }
    }

    // 更新页面表的 created_at 字段
    // 先处理标准格式的时间戳
    let sql = format!(
        "UPDATE page SET created_at = {} WHERE created_at IS NOT NULL AND created_at != '' AND datetime(created_at) IS NOT NULL",
        offset_sql.replace("{field}", "created_at")
    );

    match db.execute_unprepared(&sql).await {
        Ok(result) => {
            let rows_affected = result.rows_affected();
            updated_count += rows_affected as u32;
            debug!("更新页面表的 created_at 字段（标准格式），影响 {} 行", rows_affected);
        }
        Err(e) => {
            error!("更新页面表的 created_at 字段（标准格式）失败: {}", e);
        }
    }

    // 处理带UTC后缀的时间戳
    let utc_sql = format!(
        "UPDATE page SET created_at = {} WHERE created_at IS NOT NULL AND created_at LIKE '% UTC' AND datetime(REPLACE(created_at, ' UTC', '')) IS NOT NULL",
        offset_sql.replace("{field}", "REPLACE(created_at, ' UTC', '')")
    );

    match db.execute_unprepared(&utc_sql).await {
        Ok(result) => {
            let rows_affected = result.rows_affected();
            updated_count += rows_affected as u32;
            debug!("更新页面表的 created_at 字段（UTC格式），影响 {} 行", rows_affected);
        }
        Err(e) => {
            error!("更新页面表的 created_at 字段（UTC格式）失败: {}", e);
        }
    }

    // 更新视频源表的 latest_row_at 字段
    let tables_with_latest_row_at = vec!["favorite", "collection", "watch_later", "submission"];

    for table in tables_with_latest_row_at {
        let sql = format!(
            "UPDATE {} SET latest_row_at = {} WHERE latest_row_at IS NOT NULL AND latest_row_at != '1970-01-01 00:00:00' AND datetime(latest_row_at) IS NOT NULL",
            table,
            offset_sql.replace("{field}", "latest_row_at")
        );

        match db.execute_unprepared(&sql).await {
            Ok(result) => {
                let rows_affected = result.rows_affected();
                updated_count += rows_affected as u32;
                debug!("更新表 {} 的 latest_row_at 字段，影响 {} 行", table, rows_affected);
            }
            Err(e) => {
                error!("更新表 {} 的 latest_row_at 字段失败: {}", table, e);
            }
        }
    }

    info!("数据库时间戳同步完成，总共更新了 {} 条记录", updated_count);
    Ok(updated_count)
}

// ============================================================================
// 配置管理 API 端点
// ============================================================================

/// 获取单个配置项
#[utoipa::path(
    get,
    path = "/api/config/item/{key}",
    responses(
        (status = 200, description = "成功获取配置项", body = ConfigItemResponse),
        (status = 404, description = "配置项不存在"),
        (status = 500, description = "内部服务器错误")
    ),
    security(("Token" = []))
)]
pub async fn get_config_item(
    Path(key): Path<String>,
    Extension(db): Extension<Arc<DatabaseConnection>>,
) -> Result<ApiResponse<ConfigItemResponse>, ApiError> {
    use bili_sync_entity::entities::{config_item, prelude::ConfigItem};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    // 从数据库查找配置项
    let config_item = ConfigItem::find()
        .filter(config_item::Column::KeyName.eq(&key))
        .one(db.as_ref())
        .await
        .map_err(|e| ApiError::from(anyhow!("查询配置项失败: {}", e)))?;

    match config_item {
        Some(item) => {
            let value: serde_json::Value =
                serde_json::from_str(&item.value_json).map_err(|e| ApiError::from(anyhow!("解析配置值失败: {}", e)))?;

            let response = ConfigItemResponse {
                key: item.key_name,
                value,
                updated_at: item.updated_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            };

            Ok(ApiResponse::ok(response))
        }
        None => {
            use crate::api::error::InnerApiError;
            Err(ApiError::from(InnerApiError::BadRequest(format!(
                "配置项 '{}' 不存在",
                key
            ))))
        }
    }
}

// 删除未使用的外层函数，保留内部实现

pub async fn update_config_item_internal(
    db: Arc<DatabaseConnection>,
    key: String,
    request: UpdateConfigItemRequest,
) -> Result<ConfigItemResponse, ApiError> {
    use crate::config::ConfigManager;

    // 创建配置管理器
    let manager = ConfigManager::new(db.as_ref().clone());

    // 更新配置项
    if let Err(e) = manager.update_config_item(&key, request.value.clone()).await {
        warn!("更新配置项失败: {}", e);
        return Err(ApiError::from(anyhow!("更新配置项失败: {}", e)));
    }

    // 重新加载配置包
    if let Err(e) = crate::config::reload_config_bundle().await {
        warn!("重新加载配置包失败: {}", e);
    }

    // 返回响应
    let response = ConfigItemResponse {
        key: key.clone(),
        value: request.value,
        updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
    };

    Ok(response)
}

// 删除未使用的外层函数，保留内部实现

pub async fn batch_update_config_internal(
    db: Arc<DatabaseConnection>,
    request: BatchUpdateConfigRequest,
) -> Result<ConfigReloadResponse, ApiError> {
    use crate::config::ConfigManager;

    let manager = ConfigManager::new(db.as_ref().clone());

    // 批量更新配置项
    for (key, value) in request.items {
        if let Err(e) = manager.update_config_item(&key, value).await {
            warn!("更新配置项 '{}' 失败: {}", key, e);
            return Err(ApiError::from(anyhow!("更新配置项 '{}' 失败: {}", key, e)));
        }
    }

    // 重新加载配置包
    if let Err(e) = crate::config::reload_config_bundle().await {
        warn!("重新加载配置包失败: {}", e);
        return Err(ApiError::from(anyhow!("重新加载配置包失败: {}", e)));
    }

    let response = ConfigReloadResponse {
        success: true,
        message: "配置批量更新成功".to_string(),
        reloaded_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
    };

    Ok(response)
}

// 删除未使用的外层函数，保留内部实现

pub async fn reload_config_new_internal(_db: Arc<DatabaseConnection>) -> Result<ConfigReloadResponse, ApiError> {
    // 重新加载配置包
    if let Err(e) = crate::config::reload_config_bundle().await {
        warn!("重新加载配置包失败: {}", e);
        return Err(ApiError::from(anyhow!("重新加载配置包失败: {}", e)));
    }

    let response = ConfigReloadResponse {
        success: true,
        message: "配置重载成功".to_string(),
        reloaded_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
    };

    Ok(response)
}

/// 获取配置变更历史
#[utoipa::path(
    get,
    path = "/api/config/history",
    params(ConfigHistoryRequest),
    responses(
        (status = 200, description = "成功获取配置变更历史", body = ConfigHistoryResponse),
        (status = 500, description = "内部服务器错误")
    ),
    security(("Token" = []))
)]
pub async fn get_config_history(
    Query(params): Query<ConfigHistoryRequest>,
    Extension(db): Extension<Arc<DatabaseConnection>>,
) -> Result<ApiResponse<ConfigHistoryResponse>, ApiError> {
    use crate::config::ConfigManager;

    let manager = ConfigManager::new(db.as_ref().clone());

    let changes = manager
        .get_config_history(params.key.as_deref(), params.limit)
        .await
        .map_err(|e| ApiError::from(anyhow!("获取配置变更历史失败: {}", e)))?;

    let change_infos: Vec<ConfigChangeInfo> = changes
        .into_iter()
        .map(|change| ConfigChangeInfo {
            id: change.id,
            key_name: change.key_name,
            old_value: change.old_value,
            new_value: change.new_value,
            changed_at: change.changed_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        })
        .collect();

    let response = ConfigHistoryResponse {
        total: change_infos.len(),
        changes: change_infos,
    };

    Ok(ApiResponse::ok(response))
}

/// 验证配置
#[utoipa::path(
    post,
    path = "/api/config/validate",
    responses(
        (status = 200, description = "配置验证结果", body = ConfigValidationResponse),
        (status = 500, description = "内部服务器错误")
    ),
    security(("Token" = []))
)]
pub async fn validate_config(
    Extension(_db): Extension<Arc<DatabaseConnection>>,
) -> Result<ApiResponse<ConfigValidationResponse>, ApiError> {
    // 使用当前配置进行验证
    let is_valid = crate::config::with_config(|bundle| bundle.validate());

    let response = ConfigValidationResponse {
        valid: is_valid,
        errors: if is_valid {
            vec![]
        } else {
            vec!["配置验证失败".to_string()]
        },
        warnings: vec![],
    };

    Ok(ApiResponse::ok(response))
}

/// 获取热重载状态
#[utoipa::path(
    get,
    path = "/api/config/hot-reload/status",
    responses(
        (status = 200, description = "热重载状态", body = HotReloadStatusResponse),
        (status = 500, description = "内部服务器错误")
    ),
    security(("Token" = []))
)]
pub async fn get_hot_reload_status(
    Extension(_db): Extension<Arc<DatabaseConnection>>,
) -> Result<ApiResponse<HotReloadStatusResponse>, ApiError> {
    // TODO: 实现真正的热重载状态检查
    let response = HotReloadStatusResponse {
        enabled: true,
        last_reload: Some(Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string()),
        pending_changes: 0,
    };

    Ok(ApiResponse::ok(response))
}

/// 检查是否需要初始设置
#[utoipa::path(
    get,
    path = "/api/setup/check",
    responses(
        (status = 200, description = "初始设置检查结果", body = InitialSetupCheckResponse),
        (status = 500, description = "内部服务器错误")
    )
)]
pub async fn check_initial_setup() -> Result<ApiResponse<InitialSetupCheckResponse>, ApiError> {
    // 使用配置包系统获取最新配置
    let (has_auth_token, has_credential) = crate::config::with_config(|bundle| {
        let config = &bundle.config;

        // 检查是否有auth_token
        let has_auth_token = config.auth_token.is_some() && !config.auth_token.as_ref().unwrap().is_empty();

        // 检查是否有凭证
        let credential = config.credential.load();
        let has_credential = match credential.as_deref() {
            Some(cred) => {
                !cred.sessdata.is_empty()
                    && !cred.bili_jct.is_empty()
                    && !cred.buvid3.is_empty()
                    && !cred.dedeuserid.is_empty()
            }
            None => false,
        };

        (has_auth_token, has_credential)
    });

    // 如果没有auth_token，则需要初始设置
    let needs_setup = !has_auth_token;

    let response = InitialSetupCheckResponse {
        needs_setup,
        has_auth_token,
        has_credential,
    };

    Ok(ApiResponse::ok(response))
}

/// 设置API Token（初始设置）
#[utoipa::path(
    post,
    path = "/api/setup/auth-token",
    request_body = SetupAuthTokenRequest,
    responses(
        (status = 200, description = "API Token设置成功", body = SetupAuthTokenResponse),
        (status = 400, description = "请求参数错误", body = String),
        (status = 500, description = "服务器内部错误", body = String)
    )
)]
pub async fn setup_auth_token(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    axum::Json(params): axum::Json<crate::api::request::SetupAuthTokenRequest>,
) -> Result<ApiResponse<crate::api::response::SetupAuthTokenResponse>, ApiError> {
    if params.auth_token.trim().is_empty() {
        return Err(ApiError::from(anyhow!("API Token不能为空")));
    }

    // 更新配置中的auth_token
    let mut config = crate::config::reload_config();
    config.auth_token = Some(params.auth_token.clone());

    // 移除配置文件保存 - 配置现在完全基于数据库
    // config.save().map_err(|e| ApiError::from(anyhow!("保存配置失败: {}", e)))?;

    // 检查是否正在扫描，如果是则通过任务队列处理
    if crate::task::is_scanning() {
        // 将配置更新任务加入队列
        use uuid::Uuid;
        let reload_task = crate::task::ReloadConfigTask {
            task_id: Uuid::new_v4().to_string(),
        };
        crate::task::enqueue_reload_task(reload_task, &db).await?;
        info!("检测到正在扫描，API Token保存任务已加入队列");
    } else {
        // 直接保存配置到数据库
        use crate::config::ConfigManager;
        let manager = ConfigManager::new(db.as_ref().clone());
        if let Err(e) = manager.save_config(&config).await {
            warn!("保存配置到数据库失败: {}", e);
        } else {
            info!("API Token已保存到数据库");
        }

        // 重新加载全局配置包（从数据库）
        if let Err(e) = crate::config::reload_config_bundle().await {
            warn!("重新加载配置包失败: {}", e);
            // 回退到传统的重新加载方式
            crate::config::reload_config();
        }
    }

    let response = crate::api::response::SetupAuthTokenResponse {
        success: true,
        message: "API Token设置成功".to_string(),
    };

    Ok(ApiResponse::ok(response))
}

/// 更新B站登录凭证
#[utoipa::path(
    put,
    path = "/api/credential",
    request_body = UpdateCredentialRequest,
    responses(
        (status = 200, description = "凭证更新成功", body = UpdateCredentialResponse),
        (status = 400, description = "请求参数错误", body = String),
        (status = 500, description = "服务器内部错误", body = String)
    )
)]
pub async fn update_credential(
    Extension(db): Extension<Arc<DatabaseConnection>>,
    axum::Json(params): axum::Json<crate::api::request::UpdateCredentialRequest>,
) -> Result<ApiResponse<crate::api::response::UpdateCredentialResponse>, ApiError> {
    // 验证必填字段
    if params.sessdata.trim().is_empty()
        || params.bili_jct.trim().is_empty()
        || params.buvid3.trim().is_empty()
        || params.dedeuserid.trim().is_empty()
    {
        return Err(ApiError::from(anyhow!("请填写所有必需的凭证信息")));
    }

    // 创建新的凭证
    let new_credential = crate::bilibili::Credential {
        sessdata: params.sessdata.trim().to_string(),
        bili_jct: params.bili_jct.trim().to_string(),
        buvid3: params.buvid3.trim().to_string(),
        dedeuserid: params.dedeuserid.trim().to_string(),
        ac_time_value: params.ac_time_value.unwrap_or_default().trim().to_string(),
    };

    // 更新配置中的凭证
    let config = crate::config::reload_config();
    config.credential.store(Some(std::sync::Arc::new(new_credential)));

    // 移除配置文件保存 - 配置现在完全基于数据库
    // config.save().map_err(|e| ApiError::from(anyhow!("保存配置失败: {}", e)))?;

    // 检查是否正在扫描，如果是则通过任务队列处理
    if crate::task::is_scanning() {
        // 将配置更新任务加入队列
        use uuid::Uuid;
        let reload_task = crate::task::ReloadConfigTask {
            task_id: Uuid::new_v4().to_string(),
        };
        crate::task::enqueue_reload_task(reload_task, &db).await?;
        info!("检测到正在扫描，凭证保存任务已加入队列");
    } else {
        // 直接保存配置到数据库
        use crate::config::ConfigManager;
        let manager = ConfigManager::new(db.as_ref().clone());
        if let Err(e) = manager.save_config(&config).await {
            warn!("保存配置到数据库失败: {}", e);
        } else {
            info!("凭证已保存到数据库");
        }

        // 重新加载全局配置包（从数据库）
        if let Err(e) = crate::config::reload_config_bundle().await {
            warn!("重新加载配置包失败: {}", e);
            // 回退到传统的重新加载方式
            crate::config::reload_config();
        }
    }

    let response = crate::api::response::UpdateCredentialResponse {
        success: true,
        message: "B站凭证更新成功".to_string(),
    };

    Ok(ApiResponse::ok(response))
}

/// 暂停扫描功能
#[utoipa::path(
    post,
    path = "/api/task-control/pause",
    responses(
        (status = 200, description = "暂停成功", body = crate::api::response::TaskControlResponse),
        (status = 500, description = "内部错误")
    )
)]
pub async fn pause_scanning_endpoint() -> Result<ApiResponse<crate::api::response::TaskControlResponse>, ApiError> {
    crate::task::pause_scanning().await;
    Ok(ApiResponse::ok(crate::api::response::TaskControlResponse {
        success: true,
        message: "已暂停所有扫描和下载任务".to_string(),
        is_paused: true,
    }))
}

/// 恢复扫描功能
#[utoipa::path(
    post,
    path = "/api/task-control/resume",
    responses(
        (status = 200, description = "恢复成功", body = crate::api::response::TaskControlResponse),
        (status = 500, description = "内部错误")
    )
)]
pub async fn resume_scanning_endpoint() -> Result<ApiResponse<crate::api::response::TaskControlResponse>, ApiError> {
    crate::task::resume_scanning();
    Ok(ApiResponse::ok(crate::api::response::TaskControlResponse {
        success: true,
        message: "已恢复所有扫描和下载任务".to_string(),
        is_paused: false,
    }))
}

/// 获取任务控制状态
#[utoipa::path(
    get,
    path = "/api/task-control/status",
    responses(
        (status = 200, description = "获取状态成功", body = crate::api::response::TaskControlStatusResponse),
        (status = 500, description = "内部错误")
    )
)]
pub async fn get_task_control_status() -> Result<ApiResponse<crate::api::response::TaskControlStatusResponse>, ApiError>
{
    let is_paused = crate::task::TASK_CONTROLLER.is_paused();
    let is_scanning = crate::task::TASK_CONTROLLER.is_scanning();

    Ok(ApiResponse::ok(crate::api::response::TaskControlStatusResponse {
        is_paused,
        is_scanning,
        message: if is_paused {
            "任务已暂停".to_string()
        } else if is_scanning {
            "正在扫描中".to_string()
        } else {
            "任务空闲".to_string()
        },
    }))
}

/// 获取视频播放信息（在线播放用）
#[utoipa::path(
    get,
    path = "/api/videos/{video_id}/play-info",
    params(
        ("video_id" = String, Path, description = "视频ID或分页ID")
    ),
    responses(
        (status = 200, description = "获取播放信息成功", body = crate::api::response::VideoPlayInfoResponse),
        (status = 404, description = "视频不存在"),
        (status = 500, description = "内部错误")
    )
)]
pub async fn get_video_play_info(
    Path(video_id): Path<String>,
    Extension(db): Extension<Arc<DatabaseConnection>>,
) -> Result<ApiResponse<crate::api::response::VideoPlayInfoResponse>, ApiError> {
    use crate::api::response::{AudioStreamInfo, SubtitleStreamInfo, VideoPlayInfoResponse, VideoStreamInfo};
    use crate::bilibili::{BestStream, BiliClient, FilterOption, PageInfo, Stream, Video};

    // 查找视频信息
    let video_info = find_video_info(&video_id, &db)
        .await
        .map_err(|e| ApiError::from(anyhow!("获取视频信息失败: {}", e)))?;

    debug!(
        "获取视频播放信息: bvid={}, aid={}, cid={}, source_type={:?}, ep_id={:?}",
        video_info.bvid, video_info.aid, video_info.cid, video_info.source_type, video_info.ep_id
    );

    // 创建B站客户端
    let config = crate::config::reload_config();
    let credential = config.credential.load();
    let cookie_string = credential
        .as_ref()
        .map(|cred| {
            format!(
                "SESSDATA={};bili_jct={};buvid3={};DedeUserID={};ac_time_value={}",
                cred.sessdata, cred.bili_jct, cred.buvid3, cred.dedeuserid, cred.ac_time_value
            )
        })
        .unwrap_or_default();
    let bili_client = BiliClient::new(cookie_string);

    // 创建Video实例
    let video = Video::new_with_aid(&bili_client, video_info.bvid.clone(), video_info.aid.clone());

    // 获取分页信息
    let page_info = PageInfo {
        cid: video_info
            .cid
            .parse()
            .map_err(|_| ApiError::from(anyhow!("无效的CID")))?,
        page: 1,
        name: video_info.title.clone(),
        duration: 0,
        first_frame: None,
        dimension: None,
    };

    // 获取视频播放链接 - 根据视频类型选择不同的API
    let mut page_analyzer = if video_info.source_type == Some(1) && video_info.ep_id.is_some() {
        // 使用番剧专用API
        let ep_id = video_info.ep_id.as_ref().unwrap();
        debug!("API播放使用番剧专用API: ep_id={}", ep_id);
        video
            .get_bangumi_page_analyzer(&page_info, ep_id)
            .await
            .map_err(|e| ApiError::from(anyhow!("获取番剧视频分析器失败: {}", e)))?
    } else {
        // 使用普通视频API
        video
            .get_page_analyzer(&page_info)
            .await
            .map_err(|e| ApiError::from(anyhow!("获取视频分析器失败: {}", e)))?
    };

    // 使用默认的筛选选项
    let filter_option = FilterOption::default();
    let best_stream = page_analyzer
        .best_stream(&filter_option)
        .map_err(|e| ApiError::from(anyhow!("获取最佳视频流失败: {}", e)))?;

    debug!(
        "获取到的流类型: {:?}",
        match &best_stream {
            BestStream::VideoAudio { .. } => "DASH视频+音频分离流",
            BestStream::Mixed(_) => "混合流（包含音频）",
        }
    );

    let mut video_streams = Vec::new();
    let mut audio_streams = Vec::new();

    match best_stream {
        BestStream::VideoAudio {
            video: video_stream,
            audio: audio_stream,
        } => {
            // 使用与下载流程相同的方式获取URL
            let video_urls = video_stream.urls();

            // 处理视频流 - 使用第一个可用URL作为主URL，其余作为备用
            if let Some((main_url, backup_urls)) = video_urls.split_first() {
                if let Stream::DashVideo { quality, codecs, .. } = &video_stream {
                    video_streams.push(VideoStreamInfo {
                        url: main_url.to_string(),
                        backup_urls: backup_urls.iter().map(|s| s.to_string()).collect(),
                        quality: *quality as u32,
                        quality_description: get_video_quality_description(*quality),
                        codecs: get_video_codecs_description(*codecs),
                        width: None,
                        height: None,
                    });
                }
            }

            // 处理音频流
            if let Some(audio_stream) = audio_stream {
                let audio_urls = audio_stream.urls();
                if let Some((main_url, backup_urls)) = audio_urls.split_first() {
                    if let Stream::DashAudio { quality, .. } = &audio_stream {
                        audio_streams.push(AudioStreamInfo {
                            url: main_url.to_string(),
                            backup_urls: backup_urls.iter().map(|s| s.to_string()).collect(),
                            quality: *quality as u32,
                            quality_description: get_audio_quality_description(*quality),
                        });
                    }
                }
            }
        }
        BestStream::Mixed(stream) => {
            // 处理混合流（FLV或MP4）- 使用与下载流程相同的方式
            let urls = stream.urls();
            if let Some((main_url, backup_urls)) = urls.split_first() {
                video_streams.push(VideoStreamInfo {
                    url: main_url.to_string(),
                    backup_urls: backup_urls.iter().map(|s| s.to_string()).collect(),
                    quality: 0, // 混合流没有具体质量信息
                    quality_description: "混合流".to_string(),
                    codecs: "未知".to_string(),
                    width: None,
                    height: None,
                });
            }
        }
    }

    // 获取字幕信息
    let subtitle_streams = match video.get_subtitles(&page_info).await {
        Ok(subtitles) => {
            subtitles
                .into_iter()
                .map(|subtitle| SubtitleStreamInfo {
                    language: subtitle.lan.clone(),
                    language_doc: subtitle.lan.clone(), // 暂时使用language作为language_doc
                    url: format!("/api/videos/{}/subtitles/{}", video_id, subtitle.lan),
                })
                .collect()
        }
        Err(e) => {
            warn!("获取字幕失败: {}", e);
            Vec::new()
        }
    };

    let quality_desc = if !video_streams.is_empty() {
        video_streams[0].quality_description.clone()
    } else {
        "未知".to_string()
    };

    Ok(ApiResponse::ok(VideoPlayInfoResponse {
        success: true,
        video_streams,
        audio_streams,
        subtitle_streams,
        video_title: video_info.title,
        video_duration: Some(page_info.duration),
        video_quality_description: quality_desc,
    }))
}

/// 查找视频信息
#[derive(Debug)]
struct VideoPlayInfo {
    bvid: String,
    aid: String,
    cid: String,
    title: String,
    source_type: Option<i32>,
    ep_id: Option<String>,
}

async fn find_video_info(video_id: &str, db: &DatabaseConnection) -> Result<VideoPlayInfo> {
    use crate::bilibili::bvid_to_aid;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    // 首先尝试作为分页ID查找
    if let Ok(page_id) = video_id.parse::<i32>() {
        if let Some(page_record) = page::Entity::find_by_id(page_id)
            .one(db)
            .await
            .context("查询分页记录失败")?
        {
            // 通过分页查找对应的视频
            if let Some(video_record) = video::Entity::find_by_id(page_record.video_id)
                .one(db)
                .await
                .context("查询视频记录失败")?
            {
                return Ok(VideoPlayInfo {
                    bvid: video_record.bvid.clone(),
                    aid: bvid_to_aid(&video_record.bvid).to_string(),
                    cid: page_record.cid.to_string(),
                    title: format!("{} - {}", video_record.name, page_record.name),
                    source_type: video_record.source_type,
                    ep_id: video_record.ep_id,
                });
            }
        }
    }

    // 尝试解析为视频ID
    let video_model = if let Ok(id) = video_id.parse::<i32>() {
        video::Entity::find_by_id(id)
            .one(db)
            .await
            .context("查询视频记录失败")?
    } else {
        // 按BVID查找
        video::Entity::find()
            .filter(video::Column::Bvid.eq(video_id))
            .one(db)
            .await
            .context("查询视频记录失败")?
    };

    let video = video_model.ok_or_else(|| anyhow::anyhow!("视频记录不存在: {}", video_id))?;

    // 获取第一个分页的cid
    let first_page = page::Entity::find()
        .filter(page::Column::VideoId.eq(video.id))
        .one(db)
        .await
        .context("查询视频分页失败")?
        .ok_or_else(|| anyhow::anyhow!("视频没有分页信息"))?;

    Ok(VideoPlayInfo {
        bvid: video.bvid.clone(),
        aid: bvid_to_aid(&video.bvid).to_string(),
        cid: first_page.cid.to_string(),
        title: video.name,
        source_type: video.source_type,
        ep_id: video.ep_id,
    })
}

/// 获取视频质量描述
fn get_video_quality_description(quality: crate::bilibili::VideoQuality) -> String {
    use crate::bilibili::VideoQuality;
    match quality {
        VideoQuality::Quality360p => "360P".to_string(),
        VideoQuality::Quality480p => "480P".to_string(),
        VideoQuality::Quality720p => "720P".to_string(),
        VideoQuality::Quality1080p => "1080P".to_string(),
        VideoQuality::Quality1080pPLUS => "1080P+".to_string(),
        VideoQuality::Quality1080p60 => "1080P60".to_string(),
        VideoQuality::Quality4k => "4K".to_string(),
        VideoQuality::QualityHdr => "HDR".to_string(),
        VideoQuality::QualityDolby => "杜比视界".to_string(),
        VideoQuality::Quality8k => "8K".to_string(),
    }
}

/// 获取音频质量描述
fn get_audio_quality_description(quality: crate::bilibili::AudioQuality) -> String {
    use crate::bilibili::AudioQuality;
    match quality {
        AudioQuality::Quality64k => "64K".to_string(),
        AudioQuality::Quality132k => "132K".to_string(),
        AudioQuality::Quality192k => "192K".to_string(),
        AudioQuality::QualityDolby => "杜比全景声".to_string(),
        AudioQuality::QualityHiRES => "Hi-Res无损".to_string(),
    }
}

/// 获取视频编码描述
fn get_video_codecs_description(codecs: crate::bilibili::VideoCodecs) -> String {
    use crate::bilibili::VideoCodecs;
    match codecs {
        VideoCodecs::AVC => "AVC/H.264".to_string(),
        VideoCodecs::HEV => "HEVC/H.265".to_string(),
        VideoCodecs::AV1 => "AV1".to_string(),
    }
}

/// 代理B站视频流（解决跨域和防盗链）
#[utoipa::path(
    get,
    path = "/api/videos/proxy-stream",
    params(
        ("url" = String, Query, description = "要代理的视频流URL"),
        ("referer" = Option<String>, Query, description = "可选的Referer头")
    ),
    responses(
        (status = 200, description = "视频流代理成功"),
        (status = 400, description = "参数错误"),
        (status = 500, description = "代理失败")
    )
)]
pub async fn proxy_video_stream(
    Query(params): Query<std::collections::HashMap<String, String>>,
    headers: axum::http::HeaderMap,
) -> impl axum::response::IntoResponse {
    use axum::http::{header, HeaderValue, StatusCode};
    use axum::response::{IntoResponse, Response};

    let stream_url = match params.get("url") {
        Some(url) => url,
        None => {
            return (StatusCode::BAD_REQUEST, "缺少url参数").into_response();
        }
    };

    debug!("代理视频流请求: {}", stream_url);

    // 检查认证信息
    let config = crate::config::reload_config();
    let credential = config.credential.load();
    debug!("当前认证信息是否存在: {}", credential.is_some());
    if let Some(cred) = credential.as_ref() {
        debug!(
            "认证信息详情: SESSDATA={}, bili_jct={}, DedeUserID={}",
            &cred.sessdata[..10],
            &cred.bili_jct[..10],
            cred.dedeuserid
        );
    }

    // 使用与下载器相同的方式：只需要正确的默认头，不需要cookie认证
    debug!("使用与下载器相同的方式访问视频流，不添加cookie认证");

    // 检查Range请求
    let range_header = headers.get(header::RANGE).and_then(|h| h.to_str().ok());

    // 使用与下载器相同的Client设置进行流式代理
    let bili_client = crate::bilibili::BiliClient::new(String::new());
    let mut request_builder = bili_client.client.request(reqwest::Method::GET, stream_url, None);

    // 如果有Range请求，转发它
    if let Some(range) = range_header {
        request_builder = request_builder.header(header::RANGE, range);
    }

    // 发送请求
    let response = match request_builder.send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("代理请求失败: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "代理请求失败").into_response();
        }
    };

    let status = response.status();
    let response_headers = response.headers().clone();

    debug!("B站视频流响应状态: {}", status);
    debug!("B站视频流响应头: {:?}", response_headers);

    // 如果是401错误，记录更多详细信息
    if status == reqwest::StatusCode::UNAUTHORIZED {
        error!("B站视频流返回401未授权错误");
        error!("请求URL: {}", stream_url);
        error!("使用下载器模式: 无cookie认证");
        return (StatusCode::UNAUTHORIZED, "B站视频流未授权").into_response();
    }

    // 如果是其他错误，也记录
    if !status.is_success() {
        error!("B站视频流返回错误状态: {}", status);
        return (
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            "B站视频流请求失败",
        )
            .into_response();
    }

    // 获取响应体
    // 获取响应流而不是一次性读取所有字节
    let body_stream = response.bytes_stream();

    // 构建流式响应
    let mut proxy_response = Response::new(axum::body::Body::from_stream(body_stream));
    *proxy_response.status_mut() = status;

    let proxy_headers = proxy_response.headers_mut();

    // 复制重要的响应头
    for (key, value) in response_headers.iter() {
        match key.as_str() {
            "content-type" | "content-length" | "content-range" | "accept-ranges" => {
                proxy_headers.insert(key, value.clone());
            }
            _ => {}
        }
    }

    // 添加CORS头
    proxy_headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    proxy_headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, HEAD, OPTIONS"),
    );
    proxy_headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Range"));

    // 设置缓存控制
    proxy_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"));

    debug!("返回流式响应，状态码: {}", status);
    proxy_response
}

/// 四步法安全重命名目录，避免父子目录冲突
/// 生成唯一的文件夹名称，避免同名冲突
fn generate_unique_folder_name(parent_dir: &std::path::Path, base_name: &str, bvid: &str, pubtime: &str) -> String {
    let mut unique_name = base_name.to_string();
    let mut counter = 0;

    // 检查基础名称是否已存在
    let base_path = parent_dir.join(&unique_name);
    if !base_path.exists() {
        return unique_name;
    }

    // 如果存在，先尝试追加发布时间
    unique_name = format!("{}-{}", base_name, pubtime);
    let time_path = parent_dir.join(&unique_name);
    if !time_path.exists() {
        info!("检测到文件夹名冲突，追加发布时间: {} -> {}", base_name, unique_name);
        return unique_name;
    }

    // 如果发布时间也冲突，追加BVID
    unique_name = format!("{}-{}", base_name, bvid);
    let bvid_path = parent_dir.join(&unique_name);
    if !bvid_path.exists() {
        info!("检测到文件夹名冲突，追加BVID: {} -> {}", base_name, unique_name);
        return unique_name;
    }

    // 如果都冲突，使用数字后缀
    loop {
        counter += 1;
        unique_name = format!("{}-{}", base_name, counter);
        let numbered_path = parent_dir.join(&unique_name);
        if !numbered_path.exists() {
            warn!("检测到严重文件夹名冲突，使用数字后缀: {} -> {}", base_name, unique_name);
            return unique_name;
        }

        // 防止无限循环
        if counter > 1000 {
            warn!("文件夹名冲突解决失败，使用随机后缀");
            unique_name = format!(
                "{}-{}",
                base_name,
                uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("random")
            );
            return unique_name;
        }
    }
}

/// 智能重组视频文件夹
/// 处理从共享文件夹（如按UP主分类）到独立文件夹（如按视频标题分类）的重组
// 从数据库查询并移动特定视频的所有文件到目标文件夹
async fn extract_video_files_by_database(
    db: &DatabaseConnection,
    video_id: i32,
    target_path: &std::path::Path,
) -> Result<(), std::io::Error> {
    use bili_sync_entity::prelude::*;
    use sea_orm::*;

    info!(
        "开始通过数据库查询移动视频文件到: {:?} (video_id: {})",
        target_path, video_id
    );

    // 创建目标文件夹
    std::fs::create_dir_all(target_path)?;

    // 从数据库查询所有相关页面的文件路径
    let pages = match Page::find()
        .filter(bili_sync_entity::page::Column::VideoId.eq(video_id))
        .filter(bili_sync_entity::page::Column::DownloadStatus.gt(0))
        .all(db)
        .await
    {
        Ok(pages) => pages,
        Err(e) => {
            return Err(std::io::Error::other(format!("数据库查询失败: {}", e)));
        }
    };

    if pages.is_empty() {
        info!("视频 {} 没有已下载的页面", video_id);
        return Ok(());
    }

    let mut moved_files = 0;
    let mut total_files = 0;
    let mut pages_to_update = Vec::new(); // 记录需要更新路径的页面
    let mut source_dirs_to_check = std::collections::HashSet::new(); // 记录需要检查是否为空的源目录

    // 移动每个页面的相关文件
    for page in pages {
        // 跳过没有路径信息的页面
        let page_path_str = match &page.path {
            Some(path) => path,
            None => {
                debug!("页面 {} 没有路径信息，跳过", page.id);
                continue;
            }
        };

        let page_file_path = std::path::Path::new(page_path_str);

        // 获取页面文件所在的目录
        if let Some(page_dir) = page_file_path.parent() {
            // 记录源目录，稍后检查是否需要删除
            source_dirs_to_check.insert(page_dir.to_path_buf());
            // 收集该页面的所有相关文件
            if let Ok(entries) = std::fs::read_dir(page_dir) {
                for entry in entries.flatten() {
                    let file_path = entry.path();

                    // 检查文件是否属于当前页面
                    if let Some(file_name) = file_path.file_name() {
                        let file_name_str = file_name.to_string_lossy();
                        let page_base_name = page_file_path.file_stem().unwrap_or_default().to_string_lossy();

                        // 获取原始基础名称（去除数字后缀）
                        let original_base_name = if let Some(index) = page_base_name.rfind('-') {
                            if let Some(suffix) = page_base_name.get(index + 1..) {
                                if suffix.chars().all(|c| c.is_ascii_digit()) {
                                    // 如果后缀是纯数字，说明是重复文件，使用原始名称匹配
                                    page_base_name.get(..index).unwrap_or(&page_base_name)
                                } else {
                                    &page_base_name
                                }
                            } else {
                                &page_base_name
                            }
                        } else {
                            &page_base_name
                        };

                        // 如果文件名包含原始基础名称，就认为是相关文件
                        if file_name_str.contains(original_base_name) {
                            total_files += 1;
                            let target_file = target_path.join(file_name);

                            // 避免重复移动（如果文件已经在目标位置）
                            if file_path == target_file {
                                debug!("文件已在目标位置，跳过: {:?}", file_path);
                                continue;
                            }

                            // 如果目标文件已存在，生成新的文件名避免覆盖
                            let final_target_file = if target_file.exists() {
                                generate_unique_filename_with_video_info(&target_file, video_id, db).await
                            } else {
                                target_file.clone()
                            };

                            match std::fs::rename(&file_path, &final_target_file) {
                                Ok(_) => {
                                    moved_files += 1;

                                    // **关键修复：如果移动的是页面主文件，记录需要更新数据库路径**
                                    // 检查是否为主文件：mp4或nfo文件，且文件名匹配原始基础名称
                                    let is_main_file = if let Some(extension) = file_path.extension() {
                                        let ext_str = extension.to_string_lossy().to_lowercase();
                                        (ext_str == "mp4" || ext_str == "nfo")
                                            && file_name_str.starts_with(original_base_name)
                                            && !file_name_str.contains("-fanart")
                                            && !file_name_str.contains("-poster")
                                            && !file_name_str.contains(".zh-CN.default")
                                    } else {
                                        false
                                    };

                                    if is_main_file {
                                        pages_to_update
                                            .push((page.id, final_target_file.to_string_lossy().to_string()));
                                        info!(
                                            "页面主文件移动成功，将更新数据库路径: {:?} -> {:?}",
                                            file_path, final_target_file
                                        );
                                    } else if final_target_file != target_file {
                                        info!(
                                            "移动文件成功（重命名避免覆盖）: {:?} -> {:?}",
                                            file_path, final_target_file
                                        );
                                    } else {
                                        debug!("移动文件成功: {:?} -> {:?}", file_path, final_target_file);
                                    }
                                }
                                Err(e) => {
                                    warn!("移动文件失败: {:?} -> {:?}, 错误: {}", file_path, final_target_file, e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // **关键修复：批量更新数据库中的页面路径**
    if !pages_to_update.is_empty() {
        info!("开始更新 {} 个页面的数据库路径", pages_to_update.len());

        for (page_id, new_path) in pages_to_update {
            match Page::update_many()
                .filter(bili_sync_entity::page::Column::Id.eq(page_id))
                .col_expr(bili_sync_entity::page::Column::Path, Expr::value(new_path.clone()))
                .exec(db)
                .await
            {
                Ok(_) => {
                    debug!("更新页面 {} 的数据库路径成功: {}", page_id, new_path);
                }
                Err(e) => {
                    warn!("更新页面 {} 的数据库路径失败: {}, 错误: {}", page_id, new_path, e);
                }
            }
        }

        info!("页面数据库路径更新完成");
    }

    // **清理空的源文件夹**
    let mut cleaned_dirs = 0;
    for source_dir in source_dirs_to_check {
        // 跳过目标路径，避免删除新创建的文件夹
        if source_dir == target_path {
            continue;
        }

        // 检查目录是否为空
        if let Ok(entries) = std::fs::read_dir(&source_dir) {
            let remaining_files: Vec<_> = entries.collect();
            if remaining_files.is_empty() {
                // 目录为空，尝试删除
                match std::fs::remove_dir(&source_dir) {
                    Ok(_) => {
                        cleaned_dirs += 1;
                        info!("删除空文件夹: {:?}", source_dir);
                    }
                    Err(e) => {
                        debug!("删除空文件夹失败: {:?}, 错误: {}", source_dir, e);
                    }
                }
            } else {
                debug!("源文件夹仍有 {} 个文件，保留: {:?}", remaining_files.len(), source_dir);
            }
        }
    }

    if cleaned_dirs > 0 {
        info!("清理完成：删除了 {} 个空文件夹", cleaned_dirs);
    }

    info!(
        "视频 {} 文件移动完成: 成功移动 {}/{} 个文件到 {:?}",
        video_id, moved_files, total_files, target_path
    );
    Ok(())
}

// 根据视频ID生成唯一文件名（使用发布时间或BVID后缀）
async fn generate_unique_filename_with_video_info(
    target_file: &std::path::Path,
    video_id: i32,
    db: &DatabaseConnection,
) -> std::path::PathBuf {
    let file_stem = target_file.file_stem().unwrap_or_default().to_string_lossy();
    let file_extension = target_file.extension().unwrap_or_default().to_string_lossy();
    let parent_dir = target_file.parent().unwrap_or(std::path::Path::new(""));

    // 尝试从数据库获取视频信息来生成更有意义的后缀
    let suffix = if let Ok(Some(video)) = video::Entity::find_by_id(video_id).one(db).await {
        // 优先使用发布时间
        format!("{}", video.pubtime.format("%Y-%m-%d"))
    } else {
        format!("vid{}", video_id)
    };

    let new_name = if file_extension.is_empty() {
        format!("{}-{}", file_stem, suffix)
    } else {
        format!("{}-{}.{}", file_stem, suffix, file_extension)
    };
    let new_target = parent_dir.join(new_name);

    // 如果仍然冲突，添加时间戳
    if new_target.exists() {
        let timestamp = chrono::Local::now().format("%H%M%S").to_string();
        let final_name = if file_extension.is_empty() {
            format!("{}-{}-{}", file_stem, suffix, timestamp)
        } else {
            format!("{}-{}-{}.{}", file_stem, suffix, timestamp, file_extension)
        };
        parent_dir.join(final_name)
    } else {
        new_target
    }
}

// 生成唯一文件名避免覆盖（简化版本，用于不需要数据库查询的场景）
#[allow(dead_code)]
fn generate_unique_filename(target_file: &std::path::Path) -> std::path::PathBuf {
    let file_stem = target_file.file_stem().unwrap_or_default().to_string_lossy();
    let file_extension = target_file.extension().unwrap_or_default().to_string_lossy();
    let parent_dir = target_file.parent().unwrap_or(std::path::Path::new(""));

    // 使用时间戳作为后缀
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let new_name = if file_extension.is_empty() {
        format!("{}-{}", file_stem, timestamp)
    } else {
        format!("{}-{}.{}", file_stem, timestamp, file_extension)
    };

    parent_dir.join(new_name)
}

#[allow(dead_code)]
async fn reorganize_video_folder(
    source_path: &std::path::Path,
    target_path: &std::path::Path,
    video_bvid: &str,
) -> Result<(), std::io::Error> {
    info!(
        "开始重组视频文件夹: {:?} -> {:?} (bvid: {})",
        source_path, target_path, video_bvid
    );

    // 检查源路径是否存在
    if !source_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("源文件夹不存在: {:?}", source_path),
        ));
    }

    // 如果目标路径已存在且相同，则无需重组
    if source_path == target_path {
        debug!("源路径和目标路径相同，无需重组");
        return Ok(());
    }

    // 创建目标文件夹
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 收集需要移动的文件（移动整个文件夹的所有内容）
    let mut files_to_move = Vec::new();

    if let Ok(entries) = std::fs::read_dir(source_path) {
        for entry in entries.flatten() {
            let file_path = entry.path();
            files_to_move.push(file_path);
        }
    }

    if files_to_move.is_empty() {
        warn!("源文件夹为空: {:?}", source_path);
        return Ok(());
    }

    info!("找到 {} 个文件需要移动到新位置", files_to_move.len());

    // 创建目标目录
    std::fs::create_dir_all(target_path)?;

    // 移动所有文件
    for file_path in files_to_move {
        if let Some(file_name) = file_path.file_name() {
            let target_file = target_path.join(file_name);

            // 如果目标文件已存在，生成新的文件名避免覆盖
            let final_target_file = if target_file.exists() {
                // 从文件名和扩展名中分离
                let file_stem = target_file.file_stem().unwrap_or_default().to_string_lossy();
                let file_extension = target_file.extension().unwrap_or_default().to_string_lossy();

                let mut counter = 1;
                loop {
                    let new_name = if file_extension.is_empty() {
                        format!("{}-{}", file_stem, counter)
                    } else {
                        format!("{}-{}.{}", file_stem, counter, file_extension)
                    };
                    let new_target = target_path.join(new_name);
                    if !new_target.exists() {
                        break new_target;
                    }
                    counter += 1;
                    if counter > 100 {
                        // 防止无限循环，使用随机后缀
                        let random_suffix: u32 = rand::random::<u32>() % 9000 + 1000;
                        let new_name = if file_extension.is_empty() {
                            format!("{}-{}", file_stem, random_suffix)
                        } else {
                            format!("{}-{}.{}", file_stem, random_suffix, file_extension)
                        };
                        break target_path.join(new_name);
                    }
                }
            } else {
                target_file.clone()
            };

            match std::fs::rename(&file_path, &final_target_file) {
                Ok(_) => {
                    if final_target_file != target_file {
                        info!(
                            "移动文件成功（重命名避免覆盖）: {:?} -> {:?}",
                            file_path, final_target_file
                        );
                    } else {
                        debug!("移动文件成功: {:?} -> {:?}", file_path, final_target_file);
                    }
                }
                Err(e) => {
                    warn!("移动文件失败: {:?} -> {:?}, 错误: {}", file_path, final_target_file, e);
                    // 继续处理其他文件，不因单个文件失败而终止
                }
            }
        }
    }

    // 检查源文件夹是否还有其他文件，如果为空则删除
    if let Ok(remaining_entries) = std::fs::read_dir(source_path) {
        let remaining_count = remaining_entries.count();
        if remaining_count == 0 {
            match std::fs::remove_dir(source_path) {
                Ok(_) => {
                    info!("删除空文件夹: {:?}", source_path);
                }
                Err(e) => {
                    debug!("删除空文件夹失败（可能不为空）: {:?}, 错误: {}", source_path, e);
                }
            }
        } else {
            debug!(
                "源文件夹仍有 {} 个其他文件，保留文件夹: {:?}",
                remaining_count, source_path
            );
        }
    }

    info!("重组视频文件夹完成: {} -> {:?}", video_bvid, target_path);
    Ok(())
}

#[allow(dead_code)]
async fn safe_rename_directory(old_path: &std::path::Path, new_path: &std::path::Path) -> Result<(), std::io::Error> {
    // 步骤1：记录现有模板路径
    debug!("开始四步法重命名: {:?} -> {:?}", old_path, new_path);

    if !old_path.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "源目录不存在"));
    }

    // 步骤2：使用时间戳重命名现有目录到临时名称，完全避免路径冲突
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%d_%H%M%S_%3f").to_string(); // 包含毫秒的时间戳

    let temp_name = format!("temp_rename_{}", timestamp);
    let parent_dir = old_path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "无法获取父目录"))?;
    let temp_path = parent_dir.join(&temp_name);

    // 确保临时目录名不存在
    let mut counter = 0;
    let mut final_temp_path = temp_path.clone();
    while final_temp_path.exists() {
        counter += 1;
        final_temp_path = parent_dir.join(format!("{}_{}", temp_name, counter));
        if counter > 100 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "无法生成唯一的临时目录名",
            ));
        }
    }

    debug!("步骤2: 将 {:?} 重命名为临时目录 {:?}", old_path, final_temp_path);
    std::fs::rename(old_path, &final_temp_path)?;

    // 步骤3：创建新模板目录结构
    debug!("步骤3: 创建新目录结构 {:?}", new_path);

    // 检查新路径是否需要创建父目录
    if let Some(new_parent) = new_path.parent() {
        if !new_parent.exists() {
            std::fs::create_dir_all(new_parent)?;
        }
    }

    // 步骤4：移动文件从临时目录到新目录结构中
    debug!("步骤4: 移动内容从 {:?} 到 {:?}", final_temp_path, new_path);

    // 创建最终目标目录
    std::fs::create_dir_all(new_path)?;

    // 移动所有文件和子目录
    match move_directory_contents(&final_temp_path, new_path).await {
        Ok(_) => {
            // 成功移动后，清理临时目录
            if let Err(e) = std::fs::remove_dir_all(&final_temp_path) {
                warn!("清理临时目录失败: {:?}, 错误: {}", final_temp_path, e);
            } else {
                debug!("成功清理临时目录: {:?}", final_temp_path);
            }
            Ok(())
        }
        Err(e) => {
            // 移动失败，尝试回退
            warn!("移动文件失败，尝试回退: {}", e);
            if let Err(rollback_err) = std::fs::rename(&final_temp_path, old_path) {
                error!("回退失败: {}, 原始错误: {}", rollback_err, e);
                Err(std::io::Error::other(format!(
                    "移动失败且回退失败: 移动错误={}, 回退错误={}",
                    e, rollback_err
                )))
            } else {
                debug!("成功回退到原始状态");
                Err(e)
            }
        }
    }
}

/// 移动目录内容从源目录到目标目录
#[allow(dead_code)]
async fn move_directory_contents(
    source_dir: &std::path::Path,
    target_dir: &std::path::Path,
) -> Result<(), std::io::Error> {
    let entries = std::fs::read_dir(source_dir)?;

    for entry in entries {
        let entry = entry?;
        let source_path = entry.path();
        let file_name = entry.file_name();
        let target_path = target_dir.join(&file_name);

        if source_path.is_dir() {
            // 递归移动子目录 - 使用Box::pin避免无限大小的future
            std::fs::create_dir_all(&target_path)?;
            Box::pin(move_directory_contents(&source_path, &target_path)).await?;
            std::fs::remove_dir(&source_path)?;
        } else {
            // 移动文件
            std::fs::rename(&source_path, &target_path)?;
        }
    }

    Ok(())
}

/// 番剧专用的文件移动函数，避免BVID后缀污染
async fn move_bangumi_files_to_new_path(
    video: &video::Model,
    _old_base_path: &str,
    new_base_path: &str,
    clean_empty_folders: bool,
    txn: &sea_orm::DatabaseTransaction,
) -> Result<(usize, usize), std::io::Error> {
    use std::path::Path;

    let mut moved_count = 0;
    let mut cleaned_count = 0;

    // 获取当前视频的存储路径
    let current_video_path = Path::new(&video.path);
    if !current_video_path.exists() {
        return Ok((0, 0)); // 如果视频文件夹不存在，跳过
    }

    // 使用模板重新生成视频在新基础路径下的目标路径
    let new_video_dir = Path::new(new_base_path);

    // 基于视频模型重新生成路径结构
    let new_video_path = crate::config::with_config(|bundle| {
        let video_args = crate::utils::format_arg::video_format_args(video);
        bundle.render_video_template(&video_args)
    })
    .map_err(|e| std::io::Error::other(format!("模板渲染失败: {}", e)))?;

    let target_video_dir = new_video_dir.join(&new_video_path);

    // 如果目标路径和当前路径相同，无需移动
    if current_video_path == target_video_dir {
        return Ok((0, 0));
    }

    // 使用四步重命名原则移动整个视频文件夹
    if (move_files_with_four_step_rename(
        &current_video_path.to_string_lossy(),
        &target_video_dir.to_string_lossy(),
    )
    .await)
        .is_ok()
    {
        moved_count = 1;

        // 移动成功后，执行番剧专用的文件重命名
        if let Err(e) = rename_bangumi_files_in_directory(&target_video_dir, video, txn).await {
            warn!("番剧文件重命名失败: {}", e);
        }

        // 移动成功后，检查并清理原来的父目录（如果启用了清理且为空）
        if clean_empty_folders {
            if let Some(parent_dir) = current_video_path.parent() {
                if let Ok(count) = cleanup_empty_directory(parent_dir).await {
                    cleaned_count = count;
                }
            }
        }
    }

    Ok((moved_count, cleaned_count))
}

/// 番剧文件重命名：只重命名集数部分，保留版本和后缀
async fn rename_bangumi_files_in_directory(
    video_dir: &std::path::Path,
    video: &video::Model,
    txn: &sea_orm::DatabaseTransaction,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;

    // 读取视频文件夹中的所有文件
    let entries = fs::read_dir(video_dir)?;

    // 获取相关分页信息
    let pages = page::Entity::find()
        .filter(page::Column::VideoId.eq(video.id))
        .all(txn)
        .await?;

    for entry in entries {
        let entry = entry?;
        let file_path = entry.path();

        if !file_path.is_file() {
            continue;
        }

        let old_file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

        // 解析并重命名番剧文件
        if let Some(new_file_name) = parse_and_rename_bangumi_file(&old_file_name, video, &pages) {
            if new_file_name != old_file_name {
                let new_file_path = video_dir.join(&new_file_name);

                match fs::rename(&file_path, &new_file_path) {
                    Ok(_) => {
                        debug!("番剧文件重命名成功: {} -> {}", old_file_name, new_file_name);

                        // 如果是MP4文件，更新数据库中的分页路径
                        if new_file_name.ends_with(".mp4") {
                            update_page_path_in_database(txn, &pages, &new_file_name, &new_file_path).await?;
                        }
                    }
                    Err(e) => {
                        warn!(
                            "番剧文件重命名失败: {} -> {}, 错误: {}",
                            old_file_name, new_file_name, e
                        );
                    }
                }
            }
        }
    }

    // 更新视频路径在数据库中的记录
    let video_path_str = video_dir.to_string_lossy().to_string();
    video::Entity::update_many()
        .filter(video::Column::Id.eq(video.id))
        .col_expr(video::Column::Path, Expr::value(video_path_str))
        .exec(txn)
        .await?;

    Ok(())
}

/// 解析番剧文件名并重新组合
fn parse_and_rename_bangumi_file(old_file_name: &str, video: &video::Model, pages: &[page::Model]) -> Option<String> {
    // 尝试匹配各种番剧文件名模式

    // 1. 视频级文件 (tvshow.nfo, poster.jpg, fanart.jpg)
    if matches!(old_file_name, "tvshow.nfo" | "poster.jpg" | "fanart.jpg") {
        return Some(old_file_name.to_string()); // 这些文件不需要重命名
    }

    // 2. 分页相关文件模式匹配
    // 支持的格式：S01E01-中配.mp4, S01E01-中配-poster.jpg, 第1集-日配-fanart.jpg 等
    if let Some((episode_part, suffix)) = parse_episode_file_name(old_file_name) {
        // 重新生成集数格式
        if let Some(new_episode_format) = generate_new_episode_format(video, pages, &episode_part) {
            return Some(format!("{}{}", new_episode_format, suffix));
        }
    }

    None
}

/// 解析文件名中的集数部分和后缀
fn parse_episode_file_name(file_name: &str) -> Option<(String, String)> {
    // 匹配模式：S01E01-版本-类型.扩展名 或 第X集-版本-类型.扩展名

    // 匹配 SxxExx 格式
    if let Some(captures) = regex::Regex::new(r"^(S\d{2}E\d{2})(.*)$").ok()?.captures(file_name) {
        let episode_part = captures.get(1)?.as_str().to_string();
        let suffix = captures.get(2)?.as_str().to_string();
        return Some((episode_part, suffix));
    }

    // 匹配 第X集 格式
    if let Some(captures) = regex::Regex::new(r"^(第\d+集)(.*)$").ok()?.captures(file_name) {
        let episode_part = captures.get(1)?.as_str().to_string();
        let suffix = captures.get(2)?.as_str().to_string();
        return Some((episode_part, suffix));
    }

    None
}

/// 生成新的集数格式
fn generate_new_episode_format(video: &video::Model, pages: &[page::Model], _old_episode_part: &str) -> Option<String> {
    // 如果是多P视频，使用第一个分页的信息生成新格式
    if let Some(first_page) = pages.first() {
        // 使用配置中的分页模板生成新的集数格式
        if let Ok(new_format) = crate::config::with_config(|bundle| {
            let page_args = crate::utils::format_arg::page_format_args(video, first_page);
            bundle.render_page_template(&page_args)
        }) {
            return Some(new_format);
        }
    }

    // 后备方案：使用集数信息生成
    if let Some(episode_number) = video.episode_number {
        return Some(format!("第{:02}集", episode_number));
    }

    None
}

/// 更新数据库中的分页路径
async fn update_page_path_in_database(
    txn: &sea_orm::DatabaseTransaction,
    pages: &[page::Model],
    new_file_name: &str,
    new_file_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // 查找匹配的分页记录并更新其路径
    for page_model in pages {
        // 简单匹配：如果新文件名包含分页标题或PID信息，则更新该分页的路径
        if new_file_name.contains(&page_model.name) || new_file_name.contains(&page_model.pid.to_string()) {
            page::Entity::update_many()
                .filter(page::Column::Id.eq(page_model.id))
                .col_expr(
                    page::Column::Path,
                    Expr::value(Some(new_file_path.to_string_lossy().to_string())),
                )
                .exec(txn)
                .await?;
            break;
        }
    }

    Ok(())
}

/// 验证收藏夹ID并获取收藏夹信息
#[utoipa::path(
    get,
    path = "/api/favorite/{fid}/validate",
    params(
        ("fid" = String, Path, description = "收藏夹ID"),
    ),
    responses(
        (status = 200, body = ApiResponse<crate::api::response::ValidateFavoriteResponse>),
    )
)]
pub async fn validate_favorite(
    Path(fid): Path<String>,
) -> Result<ApiResponse<crate::api::response::ValidateFavoriteResponse>, ApiError> {
    // 创建B站客户端
    let client = crate::bilibili::BiliClient::new(String::new());

    // 创建收藏夹对象
    let favorite_list = crate::bilibili::FavoriteList::new(&client, fid.clone());

    // 尝试获取收藏夹信息
    match favorite_list.get_info().await {
        Ok(info) => Ok(ApiResponse::ok(crate::api::response::ValidateFavoriteResponse {
            valid: true,
            fid: info.id,
            title: info.title,
            message: "收藏夹验证成功".to_string(),
        })),
        Err(e) => {
            warn!("验证收藏夹 {} 失败: {}", fid, e);
            Ok(ApiResponse::ok(crate::api::response::ValidateFavoriteResponse {
                valid: false,
                fid: fid.parse().unwrap_or(0),
                title: String::new(),
                message: format!("收藏夹验证失败: 可能是ID不存在或收藏夹不公开。错误详情: {}", e),
            }))
        }
    }
}

/// 获取指定UP主的收藏夹列表
#[utoipa::path(
    get,
    path = "/api/user/{uid}/favorites",
    params(
        ("uid" = i64, Path, description = "UP主ID"),
    ),
    responses(
        (status = 200, body = ApiResponse<Vec<crate::api::response::UserFavoriteFolder>>),
    )
)]
pub async fn get_user_favorites_by_uid(
    Path(uid): Path<i64>,
) -> Result<ApiResponse<Vec<crate::api::response::UserFavoriteFolder>>, ApiError> {
    // 创建B站客户端
    let client = crate::bilibili::BiliClient::new(String::new());

    // 获取指定UP主的收藏夹列表
    match client.get_user_favorite_folders(Some(uid)).await {
        Ok(folders) => {
            let response_folders: Vec<crate::api::response::UserFavoriteFolder> = folders
                .into_iter()
                .map(|f| crate::api::response::UserFavoriteFolder {
                    id: f.id,
                    fid: f.fid,
                    title: f.title,
                    media_count: f.media_count,
                })
                .collect();

            Ok(ApiResponse::ok(response_folders))
        }
        Err(e) => {
            warn!("获取UP主 {} 的收藏夹失败: {}", uid, e);
            Err(crate::api::error::InnerApiError::BadRequest(format!(
                "获取UP主收藏夹失败: 可能是UP主不存在或收藏夹不公开。错误详情: {}",
                e
            ))
            .into())
        }
    }
}
