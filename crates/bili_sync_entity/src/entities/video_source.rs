//! 视频源实体定义

use sea_orm::entity::prelude::*;
use sea_orm::ActiveModelBehavior;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Default)]
#[sea_orm(table_name = "video_source")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub path: String,
    pub r#type: i32,
    pub latest_row_at: String,
    pub created_at: String,
    pub video_name_template: Option<String>,
    pub page_name_template: Option<String>,
    pub enabled: bool,
    pub scan_deleted_videos: bool,
    pub download_danmaku: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
