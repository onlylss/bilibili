use std::path::PathBuf;

use anyhow::Result;
use tokio::fs::{self, File};

use crate::bilibili::danmaku::canvas::{CanvasConfig, DanmakuOption};
use crate::bilibili::danmaku::{AssWriter, Danmu};
use crate::bilibili::PageInfo;

pub struct DanmakuWriter<'a> {
    page: &'a PageInfo,
    danmaku: Vec<Danmu>,
}

impl<'a> DanmakuWriter<'a> {
    pub fn new(page: &'a PageInfo, danmaku: Vec<Danmu>) -> Self {
        DanmakuWriter { page, danmaku }
    }

    pub async fn write(self, path: PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        // 使用 with_config 来访问配置
        let canvas_config = crate::config::with_config(|bundle| {
            // 需要克隆 DanmakuOption 以避免生命周期问题
            let danmaku_option = bundle.config.danmaku_option.clone();
            // 使用 Box::leak 创建 'static 生命周期的引用
            let static_option: &'static DanmakuOption = Box::leak(Box::new(danmaku_option));
            CanvasConfig::new(static_option, self.page)
        });
        let mut writer =
            AssWriter::construct(File::create(path).await?, self.page.name.clone(), canvas_config.clone()).await?;
        let mut canvas = canvas_config.canvas();
        for danmuku in self.danmaku {
            if let Some(drawable) = canvas.draw(danmuku)? {
                writer.write(drawable).await?;
            }
        }
        writer.flush().await?;
        Ok(())
    }
}
