use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 为各种视频源表添加 download_danmaku 字段

        // 合集表
        manager
            .alter_table(
                Table::alter()
                    .table(Collection::Table)
                    .add_column(
                        ColumnDef::new(Collection::DownloadDanmaku)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        // 收藏夹表
        manager
            .alter_table(
                Table::alter()
                    .table(Favorite::Table)
                    .add_column(
                        ColumnDef::new(Favorite::DownloadDanmaku)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        // 投稿表
        manager
            .alter_table(
                Table::alter()
                    .table(Submission::Table)
                    .add_column(
                        ColumnDef::new(Submission::DownloadDanmaku)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        // 稍后观看表
        manager
            .alter_table(
                Table::alter()
                    .table(WatchLater::Table)
                    .add_column(
                        ColumnDef::new(WatchLater::DownloadDanmaku)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        // 视频源表（番剧）
        manager
            .alter_table(
                Table::alter()
                    .table(VideoSource::Table)
                    .add_column(
                        ColumnDef::new(VideoSource::DownloadDanmaku)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 回滚时删除字段
        manager
            .alter_table(
                Table::alter()
                    .table(Collection::Table)
                    .drop_column(Collection::DownloadDanmaku)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Favorite::Table)
                    .drop_column(Favorite::DownloadDanmaku)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Submission::Table)
                    .drop_column(Submission::DownloadDanmaku)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(WatchLater::Table)
                    .drop_column(WatchLater::DownloadDanmaku)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(VideoSource::Table)
                    .drop_column(VideoSource::DownloadDanmaku)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Collection {
    Table,
    DownloadDanmaku,
}

#[derive(DeriveIden)]
enum Favorite {
    Table,
    DownloadDanmaku,
}

#[derive(DeriveIden)]
enum Submission {
    Table,
    DownloadDanmaku,
}

#[derive(DeriveIden)]
enum WatchLater {
    Table,
    DownloadDanmaku,
}

#[derive(DeriveIden)]
enum VideoSource {
    Table,
    DownloadDanmaku,
}
