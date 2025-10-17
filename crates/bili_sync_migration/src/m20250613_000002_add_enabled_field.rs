use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 为 video_source 表添加 enabled 字段
        manager
            .alter_table(
                Table::alter()
                    .table(VideoSource::Table)
                    .add_column(
                        ColumnDef::new(VideoSource::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("视频源是否启用"),
                    )
                    .to_owned(),
            )
            .await?;

        // 为其他视频源表也添加 enabled 字段
        // 收藏夹表
        manager
            .alter_table(
                Table::alter()
                    .table(Favorite::Table)
                    .add_column(
                        ColumnDef::new(Favorite::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("收藏夹是否启用"),
                    )
                    .to_owned(),
            )
            .await?;

        // 合集表
        manager
            .alter_table(
                Table::alter()
                    .table(Collection::Table)
                    .add_column(
                        ColumnDef::new(Collection::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("合集是否启用"),
                    )
                    .to_owned(),
            )
            .await?;

        // UP主投稿表
        manager
            .alter_table(
                Table::alter()
                    .table(Submission::Table)
                    .add_column(
                        ColumnDef::new(Submission::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("UP主投稿是否启用"),
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
                        ColumnDef::new(WatchLater::Enabled)
                            .boolean()
                            .not_null()
                            .default(true)
                            .comment("稍后观看是否启用"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 删除 enabled 字段
        manager
            .alter_table(
                Table::alter()
                    .table(VideoSource::Table)
                    .drop_column(VideoSource::Enabled)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Favorite::Table)
                    .drop_column(Favorite::Enabled)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Collection::Table)
                    .drop_column(Collection::Enabled)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Submission::Table)
                    .drop_column(Submission::Enabled)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(WatchLater::Table)
                    .drop_column(WatchLater::Enabled)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum VideoSource {
    Table,
    Enabled,
}

#[derive(DeriveIden)]
enum Favorite {
    Table,
    Enabled,
}

#[derive(DeriveIden)]
enum Collection {
    Table,
    Enabled,
}

#[derive(DeriveIden)]
enum Submission {
    Table,
    Enabled,
}

#[derive(DeriveIden)]
enum WatchLater {
    Table,
    Enabled,
}
