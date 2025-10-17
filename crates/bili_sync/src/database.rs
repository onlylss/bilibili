use anyhow::Result;
use bili_sync_migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::debug;

use crate::config::CONFIG_DIR;

static GLOBAL_DB: OnceCell<Arc<DatabaseConnection>> = OnceCell::const_new();

fn database_url() -> String {
    // 确保配置目录存在
    if !CONFIG_DIR.exists() {
        std::fs::create_dir_all(&*CONFIG_DIR).expect("创建配置目录失败");
    }
    format!("sqlite://{}?mode=rwc", CONFIG_DIR.join("data.sqlite").to_string_lossy())
}

async fn database_connection() -> Result<DatabaseConnection> {
    let mut option = ConnectOptions::new(database_url());
    option
        .max_connections(20) // 降低最大连接数，避免过多连接
        .min_connections(2) // 最小连接数
        .acquire_timeout(std::time::Duration::from_secs(30)) // 缩短超时时间
        .idle_timeout(std::time::Duration::from_secs(300)) // 空闲连接超时5分钟
        .max_lifetime(std::time::Duration::from_secs(3600)) // 连接最大生命周期1小时
        .sqlx_logging(false); // 禁用sqlx查询日志，避免过多的日志输出

    let connection = Database::connect(option).await?;

    // 确保 WAL 模式已启用并应用额外的性能优化
    use sea_orm::ConnectionTrait;
    connection.execute_unprepared("PRAGMA journal_mode = WAL;").await?;
    connection.execute_unprepared("PRAGMA synchronous = NORMAL;").await?;

    // 增强内存映射配置以替代内存数据库
    connection.execute_unprepared("PRAGMA cache_size = -65536;").await?; // 64MB缓存（负值表示KB）
    connection.execute_unprepared("PRAGMA temp_store = MEMORY;").await?;
    connection.execute_unprepared("PRAGMA mmap_size = 1073741824;").await?; // 1GB内存映射
    connection.execute_unprepared("PRAGMA page_size = 4096;").await?; // 4KB页面大小

    // WAL和并发优化
    connection
        .execute_unprepared("PRAGMA wal_autocheckpoint = 1000;")
        .await?;
    connection
        .execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE);")
        .await?; // 初始化时清理WAL
    connection.execute_unprepared("PRAGMA busy_timeout = 30000;").await?; // 30秒忙等超时

    // 查询优化
    connection.execute_unprepared("PRAGMA optimize;").await?; // 启用查询优化器
    connection.execute_unprepared("PRAGMA analysis_limit = 1000;").await?; // 分析限制

    debug!("SQLite WAL 模式已启用，内存映射优化参数已应用（1GB mmap，64MB缓存）");

    Ok(connection)
}

async fn migrate_database() -> Result<()> {
    // 检查数据库文件是否存在，不存在则会在连接时自动创建
    let db_path = CONFIG_DIR.join("data.sqlite");
    if !db_path.exists() {
        debug!("数据库文件不存在，将创建新的数据库");
    } else {
        debug!("检测到现有数据库文件，将在必要时应用迁移");
    }

    // 注意此处使用内部构造的 DatabaseConnection，而不是通过 database_connection() 获取
    // 这是因为使用多个连接的 Connection 会导致奇怪的迁移顺序问题，而使用默认的连接选项不会
    let connection = Database::connect(database_url()).await?;

    // 确保所有迁移都应用
    Ok(Migrator::up(&connection, None).await?)
}

/// 预热数据库，将关键数据加载到内存映射中
async fn preheat_database(connection: &DatabaseConnection) -> Result<()> {
    use sea_orm::ConnectionTrait;
    use tracing::info;

    // 预热关键表，触发内存映射加载
    let tables = vec![
        "video",
        "page",
        "collection",
        "favorite",
        "submission",
        "watch_later",
        "video_source",
    ];

    for table in tables {
        match connection
            .execute_unprepared(&format!("SELECT COUNT(*) FROM {}", table))
            .await
        {
            Ok(result) => {
                debug!("预热表 {} 完成，行数: {:?}", table, result.rows_affected());
            }
            Err(e) => {
                debug!("预热表 {} 失败（可能不存在）: {}", table, e);
            }
        }
    }

    // 触发索引加载
    let _ = connection
        .execute_unprepared("SELECT * FROM video WHERE id > 0 LIMIT 1")
        .await;
    let _ = connection
        .execute_unprepared("SELECT * FROM page WHERE id > 0 LIMIT 1")
        .await;

    info!("数据库预热完成，关键数据已加载到内存映射");
    Ok(())
}

/// 进行数据库迁移并获取数据库连接，供外部使用
pub async fn setup_database() -> DatabaseConnection {
    migrate_database().await.expect("数据库迁移失败");
    let connection = database_connection().await.expect("获取数据库连接失败");

    // 执行番剧缓存相关的数据库迁移
    if let Err(e) = crate::utils::bangumi_cache::ensure_cache_columns(&connection).await {
        tracing::warn!("番剧缓存数据库迁移失败: {}", e);
    }

    // 预热数据库，加载热数据到内存映射
    if let Err(e) = preheat_database(&connection).await {
        tracing::warn!("数据库预热失败: {}", e);
    }

    // 设置全局数据库引用
    let connection_arc = Arc::new(connection.clone());
    let _ = GLOBAL_DB.set(connection_arc);

    connection
}

/// 获取全局数据库连接
pub fn get_global_db() -> Option<Arc<DatabaseConnection>> {
    GLOBAL_DB.get().cloned()
}
