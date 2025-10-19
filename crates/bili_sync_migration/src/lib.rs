pub use sea_orm_migration::prelude::*;

mod m20240322_000001_create_table;
mod m20240505_130850_add_collection;
mod m20240709_130914_watch_later;
mod m20240724_161008_submission;
mod m20241228_000001_add_video_query_indexes;
mod m20250104_000001_add_selected_videos_field;
mod m20250104_000002_add_auto_download_field;
mod m20250122_062926_add_latest_row_at;
mod m20250519_000001_add_source_id;
mod m20250525_000001_add_bangumi_templates;
mod m20250531_000001_fix_fid_type;
mod m20250601_000001_fix_compatibility;
mod m20250613_000001_add_performance_indexes;
mod m20250613_000002_add_enabled_field;
mod m20250616_000001_create_config_tables;
mod m20250624_000001_add_deleted_field;
mod m20250624_000002_add_scan_deleted_videos_field;
mod m20250628_000001_create_task_queue;
mod m20250708_000001_add_collection_season_structure;
mod m20250717_000001_add_staff_info;
mod m20250726_000001_unify_time_format;
mod m20250807_000001_add_video_cid;
mod m20250921_000001_add_collection_cover;
mod m20251019_000001_add_download_danmaku_field;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240322_000001_create_table::Migration),
            Box::new(m20240505_130850_add_collection::Migration),
            Box::new(m20240709_130914_watch_later::Migration),
            Box::new(m20240724_161008_submission::Migration),
            Box::new(m20250122_062926_add_latest_row_at::Migration),
            Box::new(m20250519_000001_add_source_id::Migration),
            Box::new(m20250525_000001_add_bangumi_templates::Migration),
            Box::new(m20250531_000001_fix_fid_type::Migration),
            Box::new(m20250601_000001_fix_compatibility::Migration),
            Box::new(m20250613_000001_add_performance_indexes::Migration),
            Box::new(m20250613_000002_add_enabled_field::Migration),
            Box::new(m20250616_000001_create_config_tables::Migration),
            Box::new(m20250624_000001_add_deleted_field::Migration),
            Box::new(m20250624_000002_add_scan_deleted_videos_field::Migration),
            Box::new(m20250628_000001_create_task_queue::Migration),
            Box::new(m20241228_000001_add_video_query_indexes::Migration),
            Box::new(m20250708_000001_add_collection_season_structure::Migration),
            Box::new(m20250104_000001_add_selected_videos_field::Migration),
            Box::new(m20250104_000002_add_auto_download_field::Migration),
            Box::new(m20250717_000001_add_staff_info::Migration),
            Box::new(m20250726_000001_unify_time_format::Migration),
            Box::new(m20250807_000001_add_video_cid::Migration),
            Box::new(m20250921_000001_add_collection_cover::Migration),
            Box::new(m20251019_000001_add_download_danmaku_field::Migration),
        ]
    }
}
