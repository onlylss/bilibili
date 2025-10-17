# bili-sync

![bili-sync](https://socialify.git.ci/qq1582185982/bili-sync-01/image?description=1&font=KoHo&issues=1&language=1&logo=https%3A%2F%2Fs2.loli.net%2F2023%2F12%2F02%2F9EwT2yInOu1d3zm.png&name=1&owner=1&pattern=Signal&pulls=1&stargazers=1&theme=Light)

[![GitHub Release](https://img.shields.io/github/v/release/qq1582185982/bili-sync-01)](https://github.com/qq1582185982/bili-sync-01/releases/latest)
[![Test](https://github.com/qq1582185982/bili-sync-01/actions/workflows/test.yml/badge.svg)](https://github.com/qq1582185982/bili-sync-01/actions/workflows/test.yml)
[![Release](https://github.com/qq1582185982/bili-sync-01/actions/workflows/release.yml/badge.svg)](https://github.com/qq1582185982/bili-sync-01/actions/workflows/release.yml)
[![Downloads](https://img.shields.io/github/downloads/qq1582185982/bili-sync-01/total)](https://github.com/qq1582185982/bili-sync-01/releases)

专为 NAS 用户打造的哔哩哔哩同步工具，基于 Rust & Tokio 构建。

📚 [在线文档](https://qq1582185982.github.io/bili-sync-01/) | 🚀 [快速开始](#快速开始) | 📝 [更新日志](./docs/changelog.md)

## ✨ 核心特性

### 🎯 智能化功能
- **充电视频智能识别** - 自动检测并处理充电专享视频，无需人工干预
- **失败任务智能筛选** - 一键筛选失败任务，快速定位问题
- **任务队列持久化** - 程序重启后自动恢复任务状态
- **配置热重载** - 修改配置立即生效，无需重启

### 🎬 视频源支持
- **收藏夹** - 直接显示用户所有收藏夹，支持快速选择
- **UP主投稿** - 输入UP主ID查看所有合集/系列
- **稍后再看** - 自动同步稍后再看列表
- **番剧下载** - 支持单季和全季下载模式

### 🚀 技术优势
- **高性能** - Rust + Tokio 异步架构，支持高并发
- **内存优化** - 智能内存数据库模式，提升扫描性能
- **Web管理** - 友好的 Web 界面，无需命令行操作
- **自动重试** - 智能错误处理和重试机制

## 🚀 快速开始

### 使用 Docker（推荐）

```bash
docker run -d \
  --name bili-sync \
  -p 12345:12345 \
  -v ./data:/data \
  qq1582185982/bili-sync-01:latest
```

### 二进制文件

从 [Releases](https://github.com/qq1582185982/bili-sync-01/releases) 下载对应平台的可执行文件。

### 开发环境

```bash
# 克隆项目
git clone https://github.com/qq1582185982/bili-sync-01
cd bili-sync-01

# 安装依赖并启动
./make.bat setup
./make.bat dev
```

访问 `http://localhost:12345` 进入管理界面。

## 📸 界面预览

<details>
<summary>点击展开截图</summary>

### 管理界面
![概览](./docs/assets/overview.webp)

### 视频详情
![详情](./docs/assets/detail.webp)

### 文件结构
![文件](./docs/assets/dir.webp)

</details>

## 🛠️ 配置说明

首次启动会自动进入设置向导，引导您完成：
- Cookie 配置
- 下载路径设置
- 视频源添加

所有配置支持在 Web 界面实时修改。

## 📂 项目结构

```
├── crates/                 # Rust 后端
│   ├── bili_sync/          # 主应用
│   ├── bili_sync_entity/   # 数据库实体
│   └── bili_sync_migration/# 数据库迁移
├── web/                    # Svelte 前端
├── docs/                   # VitePress 文档
└── scripts/                # 辅助脚本
```

## 🤝 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 本项目
2. 创建功能分支 (`git checkout -b feature/amazing`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送分支 (`git push origin feature/amazing`)
5. 创建 Pull Request

## 📝 许可证

本项目采用 MIT 许可证。

## 🙏 致谢

- [bilibili-API-collect](https://github.com/SocialSisterYi/bilibili-API-collect) - B站接口文档
- [bilibili-api](https://github.com/Nemo2011/bilibili-api) - Python 接口实现参考
- [danmu2ass](https://github.com/gwy15/danmu2ass) - 弹幕下载功能