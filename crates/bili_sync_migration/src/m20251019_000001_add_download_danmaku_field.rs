use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 为各种视频源表添加 download_danmaku 字段
        // 使用容错方式，如果字段已存在则跳过

        // 合集表
        let _ = db
            .execute_unprepared(
                "ALTER TABLE collection ADD COLUMN download_danmaku BOOLEAN NOT NULL DEFAULT 0",
            )
            .await;

        // 收藏夹表
        let _ = db
            .execute_unprepared(
                "ALTER TABLE favorite ADD COLUMN download_danmaku BOOLEAN NOT NULL DEFAULT 0",
            )
            .await;

        // 投稿表
        let _ = db
            .execute_unprepared(
                "ALTER TABLE submission ADD COLUMN download_danmaku BOOLEAN NOT NULL DEFAULT 0",
            )
            .await;

        // 稍后观看表
        let _ = db
            .execute_unprepared(
                "ALTER TABLE watch_later ADD COLUMN download_danmaku BOOLEAN NOT NULL DEFAULT 0",
            )
            .await;

        // 视频源表
        let _ = db
            .execute_unprepared(
                "ALTER TABLE video_source ADD COLUMN download_danmaku BOOLEAN NOT NULL DEFAULT 0",
            )
            .await;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 回滚时删除字段
        // 使用容错方式，如果字段不存在则跳过
        let _ = db
            .execute_unprepared("ALTER TABLE collection DROP COLUMN download_danmaku")
            .await;
        let _ = db
            .execute_unprepared("ALTER TABLE favorite DROP COLUMN download_danmaku")
            .await;
        let _ = db
            .execute_unprepared("ALTER TABLE submission DROP COLUMN download_danmaku")
            .await;
        let _ = db
            .execute_unprepared("ALTER TABLE watch_later DROP COLUMN download_danmaku")
            .await;
        let _ = db
            .execute_unprepared("ALTER TABLE video_source DROP COLUMN download_danmaku")
            .await;

        Ok(())
    }
}
