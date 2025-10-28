<script lang="ts">
	import api from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import { Card, CardDescription, CardHeader, CardTitle } from '$lib/components/ui/card';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { Badge } from '$lib/components/ui/badge';
	import {
		Sheet,
		SheetContent,
		SheetDescription,
		SheetFooter,
		SheetHeader,
		SheetTitle
	} from '$lib/components/ui/sheet';
	import * as Tabs from '$lib/components/ui/tabs';
	import QrLogin from '$lib/components/qr-login.svelte';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import type { ConfigResponse, VideoInfo, UserInfo } from '$lib/types';
	import {
		DownloadIcon,
		FileTextIcon,
		KeyIcon,
		MessageSquareIcon,
		MonitorIcon,
		SettingsIcon,
		ShieldIcon,
		VideoIcon,
		PaletteIcon,
		BellIcon
	} from 'lucide-svelte';
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { theme, setTheme, isDark } from '$lib/stores/theme';
	// import type { Theme } from '$lib/stores/theme'; // 未使用，已注释
	import ThemeToggle from '$lib/components/theme-toggle.svelte';

	let config: ConfigResponse | null = null;
	let loading = false;
	let saving = false;

	// 控制各个抽屉的开关状态
	let openSheet: string | null = null;

	// 随机视频封面背景
	let randomCovers: string[] = [];
	let currentBackgroundIndex = 0;

	// 获取代理后的图片URL
	function getProxiedImageUrl(originalUrl: string): string {
		if (!originalUrl) return '';
		// 使用后端代理端点
		return `/api/proxy/image?url=${encodeURIComponent(originalUrl)}`;
	}

	// 设置分类
	const settingCategories = [
		{
			id: 'naming',
			title: '文件命名',
			description: '配置视频、分页、番剧等文件命名模板',
			icon: FileTextIcon
		},
		{
			id: 'quality',
			title: '视频质量',
			description: '设置视频/音频质量、编解码器等参数',
			icon: VideoIcon
		},
		{
			id: 'download',
			title: '下载设置',
			description: '并行下载、并发控制、速率限制配置',
			icon: DownloadIcon
		},
		{
			id: 'danmaku',
			title: '弹幕设置',
			description: '弹幕显示样式和布局参数',
			icon: MessageSquareIcon
		},
		{
			id: 'credential',
			title: 'B站凭证',
			description: '配置B站登录凭证信息',
			icon: KeyIcon
		},
		{
			id: 'risk',
			title: '风控配置',
			description: 'UP主投稿获取风控策略',
			icon: ShieldIcon
		},
		{
			id: 'captcha',
			title: '验证码风控',
			description: 'v_voucher验证码风控配置',
			icon: ShieldIcon
		},
		{
			id: 'aria2',
			title: 'Aria2监控',
			description: '下载器健康检查和自动重启配置',
			icon: MonitorIcon
		},
		{
			id: 'interface',
			title: '界面设置',
			description: '主题模式、显示选项等界面配置',
			icon: PaletteIcon
		},
		{
			id: 'notification',
			title: '推送通知',
			description: '扫描完成推送、Server酱配置',
			icon: BellIcon
		},
		{
			id: 'system',
			title: '系统设置',
			description: '扫描间隔等其他设置',
			icon: SettingsIcon
		}
	];

	// 表单数据
	let videoName = '{{upper_name}}';
	let pageName = '{{pubtime}}-{{bvid}}-{{truncate title 20}}';
	let multiPageName = 'P{{pid_pad}}.{{ptitle}}';
	let bangumiName = '{{title}} S{{season_pad}}E{{pid_pad}} - {{ptitle}}';
	let folderStructure = 'Season {{season_pad}}';
	let bangumiFolderName = '{{title}}';
	let collectionFolderMode = 'unified';
	let timeFormat = '%Y-%m-%d';
	let interval = 1200;
	let nfoTimeType = 'favtime';
	let bindAddress = '0.0.0.0:12345';
	let parallelDownloadEnabled = false;
	let parallelDownloadThreads = 4;

	// 视频质量设置
	let videoMaxQuality = 'Quality8k';
	let videoMinQuality = 'Quality360p';
	let audioMaxQuality = 'QualityHiRES';
	let audioMinQuality = 'Quality64k';
	let codecs = ['AVC', 'HEV', 'AV1'];
	let noDolbyVideo = false;
	let noDolbyAudio = false;
	let noHdr = false;
	let noHires = false;

	// 弹幕设置
	let danmakuDuration = 15.0;
	let danmakuFont = '黑体';
	let danmakuFontSize = 25;
	let danmakuWidthRatio = 1.2;
	let danmakuHorizontalGap = 20.0;
	let danmakuLaneSize = 32;
	let danmakuFloatPercentage = 0.5;
	let danmakuBottomPercentage = 0.3;
	let danmakuOpacity = 76;
	let danmakuBold = true;
	let danmakuOutline = 0.8;
	let danmakuTimeOffset = 0.0;

	// 并发控制设置
	let concurrentVideo = 3;
	let concurrentPage = 2;
	let rateLimit = 4;
	let rateDuration = 250;

	// 其他设置
	let cdnSorting = false;
	let upperPath = ''; // UP主头像保存路径

	// B站凭证设置
	let sessdata = '';
	let biliJct = '';
	let buvid3 = '';
	let dedeUserId = '';
	let acTimeValue = '';
	let buvid4 = '';
	let dedeUserIdCkMd5 = '';
	let credentialSaving = false;
	let currentUser: { user_id: string; username: string; avatar_url: string } | null = null;

	// UP主投稿风控配置
	let largeSubmissionThreshold = 100;
	let baseRequestDelay = 200;
	let largeSubmissionDelayMultiplier = 2;

	// 风控验证配置
	let riskControlEnabled = false;
	let riskControlMode = 'manual';
	let riskControlTimeout = 300;
	let isSaving = false;

	// 自动验证配置
	let autoSolveService = '2captcha';
	let autoSolveApiKey = '';
	let autoSolveMaxRetries = 3;
	let autoSolveTimeout = 300;
	let enableProgressiveDelay = true;
	let maxDelayMultiplier = 4;
	let enableIncrementalFetch = true;
	let incrementalFallbackToFull = true;
	let enableBatchProcessing = false;
	let batchSize = 5;
	let batchDelaySeconds = 2;
	let enableAutoBackoff = true;
	let autoBackoffBaseSeconds = 10;
	let autoBackoffMaxMultiplier = 5;
	let sourceDelaySeconds = 2;
	let submissionSourceDelaySeconds = 5;

	// aria2监控配置
	let enableAria2HealthCheck = false;
	let enableAria2AutoRestart = false;
	let aria2HealthCheckInterval = 300;

	// 多P视频目录结构配置
	let multiPageUseSeasonStructure = false;

	// 合集目录结构配置
	let collectionUseSeasonStructure = false;

	// 番剧目录结构配置
	let bangumiUseSeasonStructure = false;

	// 推送通知配置
	let notificationEnabled = false;
	let serverchanKey = '';
	let notificationMinVideos = 1;
	let notificationSaving = false;
	let notificationStatus: {
		configured: boolean;
		enabled: boolean;
		last_notification_time: string | null;
		total_notifications_sent: number;
		last_error: string | null;
	} | null = null;

	// 显示帮助信息的状态（在文件命名抽屉中使用）
	let showHelp = false;

	// 验证相关状态
	let pageNameError = '';
	let pageNameValid = true;
	let multiPageNameError = '';
	let multiPageNameValid = true;
	let bindAddressError = '';
	let bindAddressValid = true;

	// 互斥逻辑：视频文件名模板 vs 多P视频文件名模板
	let videoNameHasPath = false;
	let multiPageNameHasPath = false;

	// 变量说明
	const variableHelp = {
		video: [
			{ name: '{{title}}', desc: '视频标题' },
			{ name: '{{show_title}}', desc: '节目标题（与title相同）' },
			{ name: '{{bvid}}', desc: 'BV号（视频编号）' },
			{ name: '{{upper_name}}', desc: 'UP主名称' },
			{ name: '{{upper_mid}}', desc: 'UP主ID' },
			{ name: '{{pubtime}}', desc: '视频发布时间' },
			{ name: '{{fav_time}}', desc: '视频收藏时间' },
			{ name: '{{ctime}}', desc: '视频创建时间' }
		],
		page: [
			{ name: '{{ptitle}}', desc: '分页标题（页面名称）' },
			{ name: '{{long_title}}', desc: '分页长标题（非番剧可用）' },
			{ name: '{{pid}}', desc: '分页页号' },
			{ name: '{{pid_pad}}', desc: '补零的分页页号（如001、002）' },
			{ name: '{{episode}}', desc: '剧集号（仅重命名可用）' },
			{ name: '{{episode_pad}}', desc: '补零的剧集号（仅重命名可用）' },
			{ name: '{{season}}', desc: '季度号（番剧/多P视频可用）' },
			{ name: '{{season_pad}}', desc: '补零的季度号（番剧/多P视频可用）' },
			{ name: '{{series_title}}', desc: '番剧系列标题（仅番剧可用）' },
			{ name: '{{version}}', desc: '番剧版本信息（仅番剧可用）' },
			{ name: '{{year}}', desc: '发布年份（番剧/多P视频可用）' },
			{ name: '{{studio}}', desc: '制作公司/UP主名称（番剧/多P视频可用）' },
			{ name: '{{actors}}', desc: '演员信息（番剧/多P视频可用）' },
			{ name: '{{share_copy}}', desc: '分享文案（番剧/多P视频可用）' },
			{ name: '{{category}}', desc: '视频分类' },
			{ name: '{{content_type}}', desc: '内容类型（仅番剧可用）' },
			{ name: '{{status}}', desc: '播出状态（仅番剧可用）' },
			{ name: '{{ep_id}}', desc: '剧集ID（仅番剧可用）' },
			{ name: '{{season_id}}', desc: '季度ID（仅番剧可用）' },
			{ name: '{{resolution}}', desc: '视频分辨率（番剧/多P视频可用）' },
			{ name: '{{duration}}', desc: '视频时长（仅重命名可用）' },
			{ name: '{{width}}', desc: '视频宽度（仅重命名可用）' },
			{ name: '{{height}}', desc: '视频高度（仅重命名可用）' }
		],
		common: [
			{ name: '{{truncate title 10}}', desc: '截取函数示例：截取标题前10个字符' },
			{ name: '路径分隔符', desc: '支持使用 / 或 \\\\ 创建子文件夹' }
		],
		time: [
			{ name: '%Y', desc: '年份（如2023）' },
			{ name: '%m', desc: '月份（如01-12）' },
			{ name: '%d', desc: '日期（如01-31）' },
			{ name: '%H', desc: '小时（如00-23）' },
			{ name: '%M', desc: '分钟（如00-59）' },
			{ name: '%S', desc: '秒数（如00-59）' }
		]
	};

	// NFO 时间类型选项
	const nfoTimeTypeOptions = [
		{ value: 'favtime', label: '收藏时间' },
		{ value: 'pubtime', label: '发布时间' }
	];

	// 视频质量选项
	const videoQualityOptions = [
		{ value: 'Quality8k', label: '8K超高清' },
		{ value: 'QualityDolby', label: '杜比视界' },
		{ value: 'QualityHdr', label: 'HDR真彩' },
		{ value: 'Quality4k', label: '4K超高清' },
		{ value: 'Quality1080p60', label: '1080P 60fps' },
		{ value: 'Quality1080pPLUS', label: '1080P+高码率' },
		{ value: 'Quality1080p', label: '1080P高清' },
		{ value: 'Quality720p', label: '720P高清' },
		{ value: 'Quality480p', label: '480P清晰' },
		{ value: 'Quality360p', label: '360P流畅' }
	];

	// 音频质量选项
	const audioQualityOptions = [
		{ value: 'QualityHiRES', label: 'Hi-Res无损' },
		{ value: 'Quality192k', label: '192K高品质' },
		{ value: 'QualityDolby', label: '杜比全景声' },
		{ value: 'Quality132k', label: '132K标准' },
		{ value: 'Quality64k', label: '64K省流' }
	];

	// 编解码器选项
	const codecOptions = [
		{ value: 'AVC', label: 'AVC/H.264' },
		{ value: 'HEV', label: 'HEVC/H.265' },
		{ value: 'AV1', label: 'AV1' }
	];

	// 响应式相关
	let innerWidth: number;
	let isMobile: boolean = false;
	let isTablet: boolean = false;
	$: isMobile = innerWidth < 768; // sm断点
	$: isTablet = innerWidth >= 768 && innerWidth < 1024; // md断点

	// 拖拽排序相关
	let draggedIndex: number | null = null;

	function handleDragStart(e: DragEvent, index: number) {
		if (e.dataTransfer) {
			draggedIndex = index;
			e.dataTransfer.effectAllowed = 'move';
			e.dataTransfer.setData('text/html', '');
		}
	}

	function handleDragOver(e: DragEvent) {
		e.preventDefault();
		if (e.dataTransfer) {
			e.dataTransfer.dropEffect = 'move';
		}
	}

	function handleDrop(e: DragEvent, dropIndex: number) {
		e.preventDefault();
		if (draggedIndex !== null && draggedIndex !== dropIndex) {
			const newCodecs = [...codecs];
			const draggedItem = newCodecs[draggedIndex];
			newCodecs.splice(draggedIndex, 1);
			newCodecs.splice(dropIndex, 0, draggedItem);
			codecs = newCodecs;
		}
		draggedIndex = null;
	}

	function removeCodec(index: number) {
		codecs = codecs.filter((_, i) => i !== index);
	}

	function handleAddCodec(e: Event) {
		const target = e.target as HTMLSelectElement;
		const value = target.value;
		if (value && !codecs.includes(value)) {
			codecs = [...codecs, value];
			target.value = '';
		}
	}

	onMount(async () => {
		setBreadcrumb([
			{ label: '主页', href: '/' },
			{ label: '设置', isActive: true }
		]);

		await loadConfig();
		await loadRandomCovers();
		// 检查当前用户信息
		await checkCurrentUser();
		// 加载推送通知状态
		await loadNotificationStatus();
		// 加载推送通知配置
		await loadNotificationConfig();
	});

	async function loadRandomCovers() {
		try {
			// 获取一些随机视频封面
			const response = await api.getVideos({ page_size: 20 });
			if (response.data && response.data.videos) {
				// 提取封面URL并过滤掉无效的，同时转换为代理URL
				randomCovers = response.data.videos
					.filter((video: VideoInfo) => video.cover && video.cover.length > 0)
					.map((video: VideoInfo) => getProxiedImageUrl(video.cover));
			}
		} catch (error) {
			console.error('Failed to load random covers:', error);
		}
	}

	// 当打开抽屉时切换背景
	$: if (openSheet && randomCovers.length > 0) {
		currentBackgroundIndex = Math.floor(Math.random() * randomCovers.length);
		console.log('Current background:', randomCovers[currentBackgroundIndex]);
	}

	async function loadConfig() {
		loading = true;
		try {
			const response = await api.getConfig();
			config = response.data;

			// 填充表单
			videoName = config.video_name || '';
			pageName = config.page_name || '';
			multiPageName = config.multi_page_name || '';
			bangumiName = config.bangumi_name || '';
			folderStructure = config.folder_structure || '';
			bangumiFolderName = config.bangumi_folder_name || '{{title}}';
			collectionFolderMode = config.collection_folder_mode || 'separate';
			timeFormat = config.time_format || '';
			interval = config.interval || 1200;
			nfoTimeType = config.nfo_time_type || 'favtime';
			bindAddress = config.bind_address || '0.0.0.0:12345';
			parallelDownloadEnabled = config.parallel_download_enabled || false;
			parallelDownloadThreads = config.parallel_download_threads || 4;

			// 视频质量设置
			videoMaxQuality = config.video_max_quality || 'Quality8k';
			videoMinQuality = config.video_min_quality || 'Quality360p';
			audioMaxQuality = config.audio_max_quality || 'QualityHiRES';
			audioMinQuality = config.audio_min_quality || 'Quality64k';
			codecs = config.codecs || ['AVC', 'HEV', 'AV1'];
			noDolbyVideo = config.no_dolby_video || false;
			noDolbyAudio = config.no_dolby_audio || false;
			noHdr = config.no_hdr || false;
			noHires = config.no_hires || false;

			// 弹幕设置
			danmakuDuration = config.danmaku_duration || 15.0;
			danmakuFont = config.danmaku_font || '黑体';
			danmakuFontSize = config.danmaku_font_size || 25;
			danmakuWidthRatio = config.danmaku_width_ratio || 1.2;
			danmakuHorizontalGap = config.danmaku_horizontal_gap || 20.0;
			danmakuLaneSize = config.danmaku_lane_size || 32;
			danmakuFloatPercentage = config.danmaku_float_percentage || 0.5;
			danmakuBottomPercentage = config.danmaku_bottom_percentage || 0.3;
			danmakuOpacity = config.danmaku_opacity || 76;
			danmakuBold = config.danmaku_bold !== undefined ? config.danmaku_bold : true;
			danmakuOutline = config.danmaku_outline || 0.8;
			danmakuTimeOffset = config.danmaku_time_offset || 0.0;

			// 并发控制设置
			concurrentVideo = config.concurrent_video || 3;
			concurrentPage = config.concurrent_page || 2;
			rateLimit = config.rate_limit || 4;
			rateDuration = config.rate_duration || 250;

			// 其他设置
			cdnSorting = config.cdn_sorting || false;
			upperPath = config.upper_path || '';

			// B站凭证设置
			sessdata = config.credential?.sessdata || '';
			biliJct = config.credential?.bili_jct || '';
			buvid3 = config.credential?.buvid3 || '';
			dedeUserId = config.credential?.dedeuserid || '';
			acTimeValue = config.credential?.ac_time_value || '';
			buvid4 = config.credential?.buvid4 || '';
			dedeUserIdCkMd5 = config.credential?.dedeuserid_ckmd5 || '';

			// UP主投稿风控配置
			largeSubmissionThreshold = config.large_submission_threshold || 100;
			baseRequestDelay = config.base_request_delay || 200;
			largeSubmissionDelayMultiplier = config.large_submission_delay_multiplier || 2;
			enableProgressiveDelay = config.enable_progressive_delay ?? true;
			maxDelayMultiplier = config.max_delay_multiplier || 4;
			enableIncrementalFetch = config.enable_incremental_fetch ?? true;
			incrementalFallbackToFull = config.incremental_fallback_to_full ?? true;
			enableBatchProcessing = config.enable_batch_processing || false;
			batchSize = config.batch_size || 5;
			batchDelaySeconds = config.batch_delay_seconds || 2;
			enableAutoBackoff = config.enable_auto_backoff ?? true;
			autoBackoffBaseSeconds = config.auto_backoff_base_seconds || 10;
			autoBackoffMaxMultiplier = config.auto_backoff_max_multiplier || 5;
			sourceDelaySeconds = config.source_delay_seconds ?? 2;
			submissionSourceDelaySeconds = config.submission_source_delay_seconds ?? 5;

			// 风控验证配置
			riskControlEnabled = config.risk_control?.enabled ?? false;
			riskControlMode = config.risk_control?.mode || 'manual';
			riskControlTimeout = config.risk_control?.timeout || 300;

			// 自动验证配置
			autoSolveService = config.risk_control?.auto_solve?.service || '2captcha';
			autoSolveApiKey = config.risk_control?.auto_solve?.api_key || '';
			autoSolveMaxRetries = config.risk_control?.auto_solve?.max_retries || 3;
			autoSolveTimeout = config.risk_control?.auto_solve?.solve_timeout || 300;

			// aria2监控配置
			enableAria2HealthCheck = config.enable_aria2_health_check ?? false;
			enableAria2AutoRestart = config.enable_aria2_auto_restart ?? false;
			aria2HealthCheckInterval = config.aria2_health_check_interval ?? 300;

			// 多P视频目录结构配置
			multiPageUseSeasonStructure = config.multi_page_use_season_structure ?? false;

			// 合集目录结构配置
			collectionUseSeasonStructure = config.collection_use_season_structure ?? false;

			// 番剧目录结构配置
			bangumiUseSeasonStructure = config.bangumi_use_season_structure ?? false;
		} catch (error: unknown) {
			console.error('加载配置失败:', error);
			toast.error('加载配置失败', {
				description: error instanceof Error ? error.message : '未知错误'
			});
		} finally {
			loading = false;
		}
	}

	// 检查模板是否包含路径
	function hasPathSeparator(value: string) {
		return value.includes('/') || value.includes('\\');
	}

	// 验证单P视频文件名模板
	function validatePageName(value: string) {
		if (value.includes('/') || value.includes('\\')) {
			pageNameError = '单P视频文件名模板不应包含路径分隔符 / 或 \\';
			pageNameValid = false;
			return false;
		}
		pageNameError = '';
		pageNameValid = true;
		return true;
	}

	// 验证多P视频文件名模板
	function validateMultiPageName(value: string) {
		if (value.includes('/') || value.includes('\\')) {
			multiPageNameError = '多P视频文件名模板不应包含路径分隔符 / 或 \\';
			multiPageNameValid = false;
			return false;
		}
		multiPageNameError = '';
		multiPageNameValid = true;
		return true;
	}

	// 验证服务器绑定地址
	function validateBindAddress(value: string) {
		const trimmedValue = value.trim();
		if (!trimmedValue) {
			bindAddressError = '绑定地址不能为空';
			bindAddressValid = false;
			return false;
		}

		// 检查是否包含端口号
		if (trimmedValue.includes(':')) {
			// 格式：IP:端口
			const parts = trimmedValue.split(':');
			if (parts.length !== 2) {
				bindAddressError = '绑定地址格式错误，应为 "IP:端口" 或 "端口"';
				bindAddressValid = false;
				return false;
			}

			const port = parseInt(parts[1]);
			if (isNaN(port) || port < 1 || port > 65535) {
				bindAddressError = '端口号必须是1-65535之间的数字';
				bindAddressValid = false;
				return false;
			}
		} else {
			// 只有端口号
			const port = parseInt(trimmedValue);
			if (isNaN(port) || port < 1 || port > 65535) {
				bindAddressError = '端口号必须是1-65535之间的数字';
				bindAddressValid = false;
				return false;
			}
		}

		bindAddressError = '';
		bindAddressValid = true;
		return true;
	}

	// 互斥逻辑处理
	function handleVideoNameChange(value: string) {
		videoNameHasPath = hasPathSeparator(value);
		if (videoNameHasPath && multiPageNameHasPath) {
			// 如果视频文件名模板设置了路径，清空多P模板中的路径
			if (multiPageName.includes('/') || multiPageName.includes('\\')) {
				// 提取文件名部分，移除路径部分
				const parts = multiPageName.split(/[/\\]/);
				multiPageName = parts[parts.length - 1] || '{{title}}-P{{pid_pad}}';
				toast.info('已自动调整多P模板', {
					description: '移除了多P模板中的路径设置，避免冲突'
				});
			}
		}
	}

	function handleMultiPageNameChange(value: string) {
		validateMultiPageName(value);
		multiPageNameHasPath = hasPathSeparator(value);
		if (multiPageNameHasPath && videoNameHasPath) {
			// 如果多P模板设置了路径，清空视频文件名模板中的路径
			if (videoName.includes('/') || videoName.includes('\\')) {
				// 提取最后一个路径组件
				const parts = videoName.split(/[/\\]/);
				videoName = parts[parts.length - 1] || '{{title}}';
				toast.info('已自动调整视频模板', {
					description: '移除了视频模板中的路径设置，避免冲突'
				});
			}
		}
	}

	// 监听变化，实时验证和处理互斥
	$: {
		if (pageName) {
			validatePageName(pageName);
		}
		if (multiPageName) {
			validateMultiPageName(multiPageName);
		}
		videoNameHasPath = hasPathSeparator(videoName);
		multiPageNameHasPath = hasPathSeparator(multiPageName);
	}

	async function saveConfig() {
		saving = true;
		try {
			// 保存前验证
			if (!validatePageName(pageName)) {
				toast.error('配置验证失败', { description: pageNameError });
				saving = false;
				return;
			}

			if (!validateMultiPageName(multiPageName)) {
				toast.error('配置验证失败', { description: multiPageNameError });
				saving = false;
				return;
			}

			if (!validateBindAddress(bindAddress)) {
				toast.error('配置验证失败', { description: bindAddressError });
				saving = false;
				return;
			}

			const params = {
				video_name: videoName,
				page_name: pageName,
				multi_page_name: multiPageName,
				bangumi_name: bangumiName,
				folder_structure: folderStructure,
				bangumi_folder_name: bangumiFolderName,
				collection_folder_mode: collectionFolderMode,
				time_format: timeFormat,
				interval: interval,
				nfo_time_type: nfoTimeType,
				bind_address: bindAddress,
				parallel_download_enabled: parallelDownloadEnabled,
				parallel_download_threads: parallelDownloadThreads,
				// 视频质量设置
				video_max_quality: videoMaxQuality,
				video_min_quality: videoMinQuality,
				audio_max_quality: audioMaxQuality,
				audio_min_quality: audioMinQuality,
				codecs: codecs,
				no_dolby_video: noDolbyVideo,
				no_dolby_audio: noDolbyAudio,
				no_hdr: noHdr,
				no_hires: noHires,
				// 弹幕设置
				danmaku_duration: danmakuDuration,
				danmaku_font: danmakuFont,
				danmaku_font_size: danmakuFontSize,
				danmaku_width_ratio: danmakuWidthRatio,
				danmaku_horizontal_gap: danmakuHorizontalGap,
				danmaku_lane_size: danmakuLaneSize,
				danmaku_float_percentage: danmakuFloatPercentage,
				danmaku_bottom_percentage: danmakuBottomPercentage,
				danmaku_opacity: danmakuOpacity,
				danmaku_bold: danmakuBold,
				danmaku_outline: danmakuOutline,
				danmaku_time_offset: danmakuTimeOffset,
				// 并发控制设置
				concurrent_video: concurrentVideo,
				concurrent_page: concurrentPage,
				rate_limit: rateLimit,
				rate_duration: rateDuration,
				// 其他设置
				cdn_sorting: cdnSorting,
				upper_path: upperPath,
				// UP主投稿风控配置
				large_submission_threshold: largeSubmissionThreshold,
				base_request_delay: baseRequestDelay,
				large_submission_delay_multiplier: largeSubmissionDelayMultiplier,
				enable_progressive_delay: enableProgressiveDelay,
				max_delay_multiplier: maxDelayMultiplier,
				enable_incremental_fetch: enableIncrementalFetch,
				incremental_fallback_to_full: incrementalFallbackToFull,
				enable_batch_processing: enableBatchProcessing,
				batch_size: batchSize,
				batch_delay_seconds: batchDelaySeconds,
				enable_auto_backoff: enableAutoBackoff,
				auto_backoff_base_seconds: autoBackoffBaseSeconds,
				auto_backoff_max_multiplier: autoBackoffMaxMultiplier,
				source_delay_seconds: sourceDelaySeconds,
				submission_source_delay_seconds: submissionSourceDelaySeconds,
				// aria2监控配置
				enable_aria2_health_check: enableAria2HealthCheck,
				enable_aria2_auto_restart: enableAria2AutoRestart,
				aria2_health_check_interval: aria2HealthCheckInterval,
				// 多P视频目录结构配置
				multi_page_use_season_structure: multiPageUseSeasonStructure,
				// 合集目录结构配置
				collection_use_season_structure: collectionUseSeasonStructure,
				// 番剧目录结构配置
				bangumi_use_season_structure: bangumiUseSeasonStructure,
				// 风控验证配置
				risk_control_enabled: riskControlEnabled,
				risk_control_mode: riskControlMode,
				risk_control_timeout: riskControlTimeout
			};

			const response = await api.updateConfig(params);

			if (response.data.success) {
				// 检查是否修改了bind_address，如果是则提醒需要重启
				if (params.bind_address && params.bind_address !== config?.bind_address) {
					toast.success('保存成功', {
						description: '端口配置已更新，请重启程序使配置生效',
						duration: 8000 // 延长显示时间
					});
				} else {
					toast.success('保存成功', { description: response.data.message });
				}
				openSheet = null; // 关闭抽屉
			} else {
				toast.error('保存失败', { description: response.data.message });
			}
		} catch (error: unknown) {
			console.error('保存配置失败:', error);
			toast.error('保存失败', { description: error instanceof Error ? error.message : '未知错误' });
		} finally {
			saving = false;
		}
	}

	async function saveCredential() {
		credentialSaving = true;
		try {
			const params = {
				sessdata: sessdata.trim(),
				bili_jct: biliJct.trim(),
				buvid3: buvid3.trim(),
				dedeuserid: dedeUserId.trim(),
				ac_time_value: acTimeValue.trim(),
				buvid4: buvid4.trim() || undefined,
				dedeuserid_ckmd5: dedeUserIdCkMd5.trim() || undefined
			};

			const response = await api.updateCredential(params);

			if (response.data.success) {
				toast.success('B站凭证保存成功', { description: response.data.message });
				// 重新加载配置以获取最新状态
				await loadConfig();
				openSheet = null; // 关闭抽屉
			} else {
				toast.error('保存失败', { description: response.data.message });
			}
		} catch (error: unknown) {
			console.error('保存B站凭证失败:', error);
			toast.error('保存失败', { description: error instanceof Error ? error.message : '未知错误' });
		} finally {
			credentialSaving = false;
		}
	}

	// 处理扫码登录成功
	async function handleQrLoginSuccess(userInfo: UserInfo) {
		// 扫码登录成功后，凭证已经在后端保存
		toast.success(`欢迎，${userInfo.username}！登录成功`);
		// 更新当前用户信息
		currentUser = userInfo;
		// 重新加载配置以获取最新凭证
		await loadConfig();
		openSheet = null; // 关闭抽屉
	}

	// 处理扫码登录错误
	function handleQrLoginError(error: string) {
		toast.error('扫码登录失败: ' + error);
	}

	// 处理退出登录
	function handleLogout() {
		// 可以在这里清除凭证，但通常用户只是想切换账号
		toast.info('请扫码登录新账号');
	}

	// 检查当前用户信息
	async function checkCurrentUser() {
		try {
			const response = await fetch('/api/auth/current-user');
			if (response.ok) {
				const result = await response.json();
				if (result.status_code === 200 && result.data) {
					currentUser = result.data;
				}
			} else {
				currentUser = null;
			}
		} catch (error) {
			console.error('检查用户信息失败:', error);
			currentUser = null;
		}
	}

	// 加载推送通知状态
	async function loadNotificationStatus() {
		try {
			const response = await api.getNotificationStatus();
			console.log('推送通知状态响应:', response);
			if (response.data) {
				notificationStatus = response.data;
				notificationEnabled = response.data.enabled;
				// min_videos 需要从配置中获取，状态API不返回这个值
				console.log('notificationStatus:', notificationStatus);
			}
		} catch (error) {
			console.error('加载推送通知状态失败:', error);
		}
	}

	// 保存推送通知配置
	async function saveNotificationConfig() {
		notificationSaving = true;
		try {
			const config: Record<string, unknown> = {
				enable_scan_notifications: notificationEnabled,
				notification_min_videos: notificationMinVideos
			};

			// 只有输入了新密钥时才更新
			if (serverchanKey.trim()) {
				config.serverchan_key = serverchanKey.trim();
			}

			const response = await api.updateNotificationConfig(config);
			// 检查响应状态码，后端返回 {status_code: 200, data: "推送配置更新成功"}
			if (response.status_code === 200) {
				toast.success('推送通知配置保存成功');
				// 重新加载状态
				await loadNotificationStatus();
				openSheet = null; // 关闭抽屉
			} else {
				toast.error('保存失败', { description: response.data || '未知错误' });
			}
		} catch (error: unknown) {
			console.error('保存推送通知配置失败:', error);
			toast.error('保存失败', { description: error instanceof Error ? error.message : '未知错误' });
		} finally {
			notificationSaving = false;
		}
	}

	async function saveRiskControlConfig() {
		isSaving = true;
		try {
			const config: UpdateConfigRequest = {
				risk_control_enabled: riskControlEnabled,
				risk_control_mode: riskControlMode,
				risk_control_timeout: riskControlTimeout,
				risk_control_auto_solve_service: autoSolveService,
				risk_control_auto_solve_api_key: autoSolveApiKey,
				risk_control_auto_solve_max_retries: autoSolveMaxRetries,
				risk_control_auto_solve_timeout: autoSolveTimeout
			};

			const response = await api.updateConfig(config);
			if (response.status_code === 200) {
				toast.success('验证码风控配置保存成功');
				// 重新加载配置以确保同步
				await loadConfig();
				openSheet = null; // 关闭抽屉
			} else {
				toast.error('保存失败', { description: response.data || '未知错误' });
			}
		} catch (error: unknown) {
			console.error('保存验证码风控配置失败:', error);
			toast.error('保存失败', { description: error instanceof Error ? error.message : '未知错误' });
		} finally {
			isSaving = false;
		}
	}

	// 加载推送通知配置
	async function loadNotificationConfig() {
		try {
			const response = await api.getNotificationConfig();
			console.log('推送通知配置响应:', response);
			if (response.data) {
				// 不覆盖密钥，只加载其他配置
				notificationEnabled = response.data.enable_scan_notifications;
				notificationMinVideos = response.data.notification_min_videos;
				console.log('加载的配置值:', {
					enabled: notificationEnabled,
					minVideos: notificationMinVideos
				});
			}
		} catch (error) {
			console.error('加载推送通知配置失败:', error);
		}
	}

	// 测试推送通知
	async function testNotification() {
		try {
			const response = await api.testNotification();
			// 检查响应状态码
			if (response.status_code === 200) {
				toast.success('测试推送发送成功', { description: '请检查您的推送接收端' });
			} else {
				toast.error('测试推送失败', { description: response.data || '未知错误' });
			}
		} catch (error: unknown) {
			console.error('测试推送失败:', error);
			toast.error('测试推送失败', {
				description: error instanceof Error ? error.message : '未知错误'
			});
		}
	}
</script>

<svelte:head>
	<title>设置 - Bili Sync</title>
</svelte:head>

<svelte:window bind:innerWidth />

<div class="py-2">
	<div class="mx-auto px-4">
		<div class="bg-card rounded-lg border shadow-sm {isMobile ? 'p-4' : 'p-6'}">
			<h1 class="font-bold {isMobile ? 'mb-4 text-xl' : 'mb-6 text-2xl'}">系统设置</h1>

			{#if loading}
				<div class="flex items-center justify-center py-12">
					<div class="text-muted-foreground">加载中...</div>
				</div>
			{:else}
				<!-- 设置分类卡片列表 -->
				<div
					class="grid gap-4 {isMobile ? 'grid-cols-1' : isTablet ? 'grid-cols-2' : 'grid-cols-3'}"
				>
					{#each settingCategories as category (category.id)}
						<Card
							class="hover:border-primary/50 cursor-pointer transition-all hover:shadow-md {isMobile
								? 'min-h-[80px]'
								: ''}"
							onclick={() => (openSheet = category.id)}
						>
							<CardHeader>
								<div class="flex {isMobile ? 'flex-col gap-2' : 'items-start gap-3'}">
									<div class="bg-primary/10 rounded-lg p-2 {isMobile ? 'self-start' : ''}">
										<svelte:component
											this={category.icon}
											class="text-primary {isMobile ? 'h-4 w-4' : 'h-5 w-5'}"
										/>
									</div>
									<div class="flex-1">
										<CardTitle class={isMobile ? 'text-sm' : 'text-base'}
											>{category.title}</CardTitle
										>
										<CardDescription class="mt-1 {isMobile ? 'text-xs' : 'text-sm'} line-clamp-2"
											>{category.description}</CardDescription
										>
									</div>
								</div>
							</CardHeader>
						</Card>
					{/each}
				</div>
			{/if}
		</div>
	</div>
</div>

<!-- 文件命名设置抽屉 -->
<Sheet
	open={openSheet === 'naming'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0" style="z-index: 0;">
				<img
					src={randomCovers[currentBackgroundIndex]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
					onerror={(e) => console.error('Image load error:', e)}
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>文件命名设置</SheetTitle>
					<SheetDescription>配置视频、分页、番剧等文件命名模板</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							/>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						saveConfig();
					}}
					class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}"
				>
					<div
						class="min-h-0 flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}"
					>
						<div class="flex items-center justify-between">
							<h3 class="text-base font-semibold">文件命名模板</h3>
							<button
								type="button"
								onclick={() => (showHelp = !showHelp)}
								class="text-sm text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300"
							>
								{showHelp ? '隐藏' : '显示'}变量说明
							</button>
						</div>

						{#if showHelp}
							<div
								class="rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-950/20"
							>
								<div
									class="grid grid-cols-1 gap-4 text-sm {isMobile
										? 'sm:grid-cols-1'
										: 'md:grid-cols-2'}"
								>
									<div>
										<h4 class="mb-2 font-medium text-blue-900 dark:text-blue-200">视频变量</h4>
										<div class="space-y-1">
											{#each variableHelp.video as item (item.name)}
												<div class="flex">
													<code
														class="mr-2 rounded bg-blue-100 px-1 text-blue-800 dark:bg-blue-900 dark:text-blue-300"
														>{item.name}</code
													>
													<span class="text-gray-600 dark:text-gray-400">{item.desc}</span>
												</div>
											{/each}
										</div>
									</div>
									<div>
										<h4 class="mb-2 font-medium text-blue-900 dark:text-blue-200">分页变量</h4>
										<div class="space-y-1">
											{#each variableHelp.page as item (item.name)}
												<div class="flex">
													<code
														class="mr-2 rounded bg-blue-100 px-1 text-blue-800 dark:bg-blue-900 dark:text-blue-300"
														>{item.name}</code
													>
													<span class="text-gray-600 dark:text-gray-400">{item.desc}</span>
												</div>
											{/each}
										</div>
										<h4 class="mt-4 mb-2 font-medium text-blue-900 dark:text-blue-200">通用函数</h4>
										<div class="space-y-1">
											{#each variableHelp.common as item (item.name)}
												<div class="flex">
													<code
														class="mr-2 rounded bg-blue-100 px-1 text-blue-800 dark:bg-blue-900 dark:text-blue-300"
														>{item.name}</code
													>
													<span class="text-gray-600 dark:text-gray-400">{item.desc}</span>
												</div>
											{/each}
										</div>
									</div>
									<div class="md:col-span-2">
										<h4 class="mb-2 font-medium text-blue-900 dark:text-blue-200">时间格式变量</h4>
										<div class="grid grid-cols-3 gap-2">
											{#each variableHelp.time as item (item.name)}
												<div class="flex">
													<code
														class="mr-2 rounded bg-blue-100 px-1 text-blue-800 dark:bg-blue-900 dark:text-blue-300"
														>{item.name}</code
													>
													<span class="text-gray-600 dark:text-gray-400">{item.desc}</span>
												</div>
											{/each}
										</div>
									</div>
								</div>
							</div>
						{/if}

						<div class="mb-4">
							<h4 class="text-lg font-medium">文件命名设置</h4>
						</div>

						<!-- 互斥提示面板 -->
						{#if videoNameHasPath && multiPageNameHasPath}
							<div
								class="mb-4 rounded-lg border border-red-200 bg-red-50 p-4 dark:border-red-800 dark:bg-red-950/20"
							>
								<h5 class="mb-2 font-medium text-red-800 dark:text-red-200">🚨 路径冲突检测</h5>
								<p class="text-sm text-red-700 dark:text-red-300">
									检测到视频文件名模板和多P视频文件名模板都设置了路径分隔符，这会导致文件夹嵌套混乱。<br
									/>
									<strong>建议：</strong>只在其中一个模板中设置路径，另一个模板只控制文件名。
								</p>
							</div>
						{/if}

						<!-- 互斥规则说明 -->
						<div
							class="mb-4 rounded-lg border border-yellow-200 bg-yellow-50 p-4 dark:border-yellow-800 dark:bg-yellow-950/20"
						>
							<h5 class="mb-2 font-medium text-yellow-800 dark:text-yellow-200">💡 智能路径管理</h5>
							<p class="text-sm text-yellow-700 dark:text-yellow-300">
								为避免文件夹嵌套混乱，系统会自动处理路径冲突：<br />
								• 当您在一个模板中设置路径时，另一个模板会自动移除路径设置<br />
								• 推荐在"视频文件名模板"中设置UP主分类，在"多P模板"中只设置文件名
							</p>
						</div>

						<div class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}">
							<div class="space-y-2">
								<Label for="video-name">视频文件名模板</Label>
								<Input
									id="video-name"
									bind:value={videoName}
									placeholder={`{{upper_name}}`}
									class={multiPageNameHasPath ? 'border-orange-400 bg-orange-50' : ''}
									oninput={(e) =>
										handleVideoNameChange((e.target as HTMLInputElement)?.value || '')}
								/>
								{#if multiPageNameHasPath && videoNameHasPath}
									<p class="text-xs text-orange-600 dark:text-orange-400">
										⚠️ 多P模板已设置路径，此模板将自动移除路径设置避免冲突
									</p>
								{/if}
								<p class="text-muted-foreground text-xs">
									控制主要文件夹结构，支持使用 / 创建子目录
								</p>
							</div>

							<div class="space-y-2">
								<Label for="page-name">单P视频文件名模板</Label>
								<Input
									id="page-name"
									bind:value={pageName}
									placeholder={`{{pubtime}}-{{bvid}}-{{truncate title 20}}`}
									class={pageNameValid ? '' : 'border-red-500 focus:border-red-500'}
								/>
								{#if pageNameError}
									<p class="text-xs text-red-500 dark:text-red-400">{pageNameError}</p>
								{/if}
								<p class="text-muted-foreground text-xs">
									控制单P视频的具体文件名，<strong>不允许使用路径分隔符 / 或 \</strong>
								</p>
							</div>

							<div class="space-y-2">
								<Label for="multi-page-name">多P视频文件名模板</Label>
								<Input
									id="multi-page-name"
									bind:value={multiPageName}
									placeholder={`P{{pid_pad}}.{{ptitle}}`}
									class={!multiPageNameValid
										? 'border-red-500 focus:border-red-500'
										: videoNameHasPath && multiPageNameHasPath
											? 'border-orange-400 bg-orange-50'
											: ''}
									oninput={(e) =>
										handleMultiPageNameChange((e.target as HTMLInputElement)?.value || '')}
								/>
								{#if multiPageNameError}
									<p class="text-xs text-red-500 dark:text-red-400">{multiPageNameError}</p>
								{/if}
								{#if !multiPageNameError && videoNameHasPath && multiPageNameHasPath}
									<p class="text-xs text-orange-600 dark:text-orange-400">
										⚠️ 检测到路径冲突：视频文件名模板和多P模板都包含路径，系统将自动调整避免冲突
									</p>
								{/if}
								<p class="text-muted-foreground text-xs">
									控制多P视频的具体文件名，<strong>不允许使用路径分隔符 / 或 \</strong>。
									如果需要目录结构，请在视频文件名模板中设置，避免与视频文件名模板冲突。
								</p>
							</div>

							<!-- 多P视频Season结构设置 -->
							<div class="space-y-2">
								<div class="flex items-center space-x-2">
									<input
										type="checkbox"
										id="multi-page-season"
										bind:checked={multiPageUseSeasonStructure}
										class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
									/>
									<Label
										for="multi-page-season"
										class="text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
									>
										多P视频使用Season文件夹结构
									</Label>
								</div>
								<p class="text-muted-foreground text-xs">
									启用后将为多P视频创建"Season 01"子文件夹，提升媒体库兼容性（如Emby/Jellyfin）
								</p>
							</div>

							<!-- 合集Season结构设置 -->
							<div class="space-y-2">
								<div class="flex items-center space-x-2">
									<input
										type="checkbox"
										id="collection-season"
										bind:checked={collectionUseSeasonStructure}
										class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
									/>
									<Label
										for="collection-season"
										class="text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
									>
										合集使用Season文件夹结构
									</Label>
								</div>
								<p class="text-muted-foreground text-xs">
									启用后将为合集创建"Season 01"子文件夹，与多P视频相同的媒体库结构
								</p>
							</div>

							<!-- 番剧Season结构设置 -->
							<div class="space-y-2">
								<div class="flex items-center space-x-2">
									<input
										type="checkbox"
										id="bangumi-season"
										bind:checked={bangumiUseSeasonStructure}
										class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
									/>
									<Label
										for="bangumi-season"
										class="text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
									>
										番剧使用统一Season文件夹结构
									</Label>
								</div>
								<p class="text-muted-foreground text-xs">
									启用后多季番剧将创建统一根目录，在其下按"Season 01"、"Season
									02"分季存放，提升媒体库识别度
								</p>
							</div>

							<div class="space-y-2">
								<Label for="bangumi-name">番剧文件名模板</Label>
								<Input id="bangumi-name" bind:value={bangumiName} placeholder={`第{{pid_pad}}集`} />
								<p class="text-muted-foreground text-xs">控制番剧的季度文件夹和集数文件名</p>
							</div>

							<div class="space-y-2">
								<Label for="bangumi-folder-name">番剧文件夹名模板</Label>
								<Input
									id="bangumi-folder-name"
									bind:value={bangumiFolderName}
									placeholder={`{{title}}`}
								/>
								<p class="text-muted-foreground text-xs">控制番剧主文件夹的命名，包含元数据文件</p>
							</div>
						</div>

						<div class="space-y-2">
							<Label for="folder-structure">文件夹结构模板</Label>
							<Input id="folder-structure" bind:value={folderStructure} placeholder="Season 1" />
							<p class="text-muted-foreground text-sm">定义视频文件的文件夹层级结构</p>
						</div>

						<div class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}">
							<div class="space-y-2">
								<Label for="collection-folder-mode">合集文件夹模式</Label>
								<select
									id="collection-folder-mode"
									bind:value={collectionFolderMode}
									class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
								>
									<option value="separate">分离模式</option>
									<option value="unified" selected>统一模式</option>
								</select>
								<p class="text-muted-foreground text-sm">
									分离模式·：每个视频独立文件夹<br />
									统一模式：所有视频在合集文件夹下
								</p>
							</div>

							<div class="space-y-2">
								<Label for="time-format">时间格式</Label>
								<Input id="time-format" bind:value={timeFormat} placeholder="%Y-%m-%d" />
								<p class="text-muted-foreground text-sm">控制时间变量的显示格式</p>
							</div>
						</div>

						<div class="space-y-2">
							<Label for="nfo-time-type">NFO文件时间类型</Label>
							<select
								id="nfo-time-type"
								bind:value={nfoTimeType}
								class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
							>
								{#each nfoTimeTypeOptions as option (option.value)}
									<option value={option.value}>{option.label}</option>
								{/each}
							</select>
							<p class="text-muted-foreground text-sm">
								选择NFO文件中使用的时间类型。
								<span class="font-medium text-amber-600">注意：</span>
								更改此设置后，系统会自动重置所有NFO相关任务状态，并立即开始重新生成NFO文件以应用新的时间类型。
							</p>
						</div>

						<!-- Season结构说明 -->
						<div
							class="mt-6 rounded-lg border border-green-200 bg-green-50 p-3 dark:border-green-800 dark:bg-green-950/20"
						>
							<h5 class="mb-2 font-medium text-green-800 dark:text-green-200">
								多P视频Season结构说明
							</h5>
							<div class="space-y-1 text-sm text-green-700 dark:text-green-300">
								<p><strong>启用后：</strong>多P视频将采用与番剧相同的目录结构</p>
								<p><strong>目录层级：</strong>视频名称/Season 01/分P文件</p>
								<p><strong>媒体库兼容：</strong>Emby/Jellyfin能正确识别为TV Show剧集</p>
								<p><strong>文件命名：</strong>保持现有的multi_page_name模板不变</p>
								<p class="text-green-600 dark:text-green-400">
									<strong>注意：</strong>默认关闭保持向后兼容，启用后新下载的多P视频将使用新结构
								</p>
							</div>
						</div>

						<div
							class="mt-6 rounded-lg border border-blue-200 bg-blue-50 p-3 dark:border-blue-800 dark:bg-blue-950/20"
						>
							<h5 class="mb-2 font-medium text-blue-800 dark:text-blue-200">番剧Season结构说明</h5>
							<div class="space-y-1 text-sm text-blue-700 dark:text-blue-300">
								<p><strong>启用后：</strong>多季番剧将创建统一的系列根目录</p>
								<p><strong>智能识别：</strong>自动从"灵笼 第二季"中提取"灵笼"作为系列名</p>
								<p><strong>目录层级：</strong>系列名/Season 01、Season 02/剧集文件</p>
								<p><strong>媒体库优势：</strong>Emby/Jellyfin能正确识别同一系列的不同季度</p>
								<p><strong>文件命名：</strong>保持现有的bangumi_name模板不变</p>
								<p class="text-blue-600 dark:text-blue-400">
									<strong>注意：</strong>默认关闭保持向后兼容，仅影响新下载的番剧
								</p>
							</div>
						</div>
					</div>
					<SheetFooter class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}>
						<Button type="submit" disabled={saving || !pageNameValid} class="w-full">
							{saving ? '保存中...' : '保存设置'}
						</Button>
						{#if !pageNameValid}
							<p class="text-center text-xs text-red-500 dark:text-red-400">
								请修复配置错误后再保存
							</p>
						{/if}
					</SheetFooter>
				</form>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- 视频质量设置抽屉 -->
<Sheet
	open={openSheet === 'quality'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 1) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>视频质量设置</SheetTitle>
					<SheetDescription>设置视频/音频质量、编解码器等参数</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							/>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						saveConfig();
					}}
					class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}"
				>
					<div class="flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}">
						<div class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}">
							<div class="space-y-2">
								<Label for="video-max-quality">视频最高质量</Label>
								<select
									id="video-max-quality"
									bind:value={videoMaxQuality}
									class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
								>
									{#each videoQualityOptions as option (option.value)}
										<option value={option.value}>{option.label}</option>
									{/each}
								</select>
							</div>

							<div class="space-y-2">
								<Label for="video-min-quality">视频最低质量</Label>
								<select
									id="video-min-quality"
									bind:value={videoMinQuality}
									class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
								>
									{#each videoQualityOptions as option (option.value)}
										<option value={option.value}>{option.label}</option>
									{/each}
								</select>
							</div>

							<div class="space-y-2">
								<Label for="audio-max-quality">音频最高质量</Label>
								<select
									id="audio-max-quality"
									bind:value={audioMaxQuality}
									class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
								>
									{#each audioQualityOptions as option (option.value)}
										<option value={option.value}>{option.label}</option>
									{/each}
								</select>
							</div>

							<div class="space-y-2">
								<Label for="audio-min-quality">音频最低质量</Label>
								<select
									id="audio-min-quality"
									bind:value={audioMinQuality}
									class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
								>
									{#each audioQualityOptions as option (option.value)}
										<option value={option.value}>{option.label}</option>
									{/each}
								</select>
							</div>
						</div>

						<div class="space-y-2">
							<Label>编解码器优先级顺序</Label>
							<p class="text-muted-foreground mb-3 text-sm">
								拖拽以调整优先级，越靠前优先级越高。根据设备硬件解码支持情况选择：
							</p>
							<div
								class="mb-3 rounded-lg border border-blue-200 bg-blue-50 p-3 dark:border-blue-800 dark:bg-blue-950/20"
							>
								<div class="space-y-2 text-xs text-blue-700 dark:text-blue-300">
									<div>
										<strong>🎯 AVC (H.264)：</strong
										>兼容性最好，几乎所有设备都支持硬件解码，播放流畅，但文件体积较大
									</div>
									<div>
										<strong>🚀 HEV (H.265)：</strong>新一代编码，体积更小，需要较新设备硬件解码支持
									</div>
									<div>
										<strong>⚡ AV1：</strong
										>最新编码格式，压缩率最高，需要最新设备支持，软解可能卡顿
									</div>
									<div class="mt-2 border-t border-blue-300 pt-1">
										<strong>💡 推荐设置：</strong
										>如果设备较老或追求兼容性，将AVC放首位；如果设备支持新编码且网络较慢，可优先HEV或AV1
									</div>
								</div>
							</div>
							<div class="space-y-2">
								{#each codecs as codec, index (codec)}
									<div
										class="flex cursor-move items-center gap-3 rounded-lg border bg-gray-50 p-3 dark:bg-gray-900"
										draggable="true"
										ondragstart={(e) => handleDragStart(e, index)}
										ondragover={handleDragOver}
										ondrop={(e) => handleDrop(e, index)}
										role="button"
										tabindex="0"
									>
										<div class="flex items-center gap-2 text-gray-400 dark:text-gray-600">
											<svg class="h-4 w-4" fill="currentColor" viewBox="0 0 20 20">
												<path
													d="M7 2a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2H7zM8 6h4v2H8V6zm0 4h4v2H8v-2z"
												/>
											</svg>
										</div>
										<div class="flex flex-1 items-center gap-2">
											<span
												class="bg-primary text-primary-foreground flex h-6 w-6 items-center justify-center rounded-full text-sm font-medium"
											>
												{index + 1}
											</span>
											<span class="font-medium">
												{codecOptions.find((option) => option.value === codec)?.label || codec}
											</span>
										</div>
										<button
											type="button"
											class="p-1 text-red-500 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300"
											onclick={() => removeCodec(index)}
											title="移除此编解码器"
											aria-label="移除此编解码器"
										>
											<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path
													stroke-linecap="round"
													stroke-linejoin="round"
													stroke-width="2"
													d="M6 18L18 6M6 6l12 12"
												/>
											</svg>
										</button>
									</div>
								{/each}

								{#if codecs.length < codecOptions.length}
									<div class="mt-2">
										<select
											class="w-full rounded-md border p-2 text-sm"
											onchange={handleAddCodec}
											value=""
										>
											<option value="" disabled>添加编解码器...</option>
											{#each codecOptions as option (option.value)}
												{#if !codecs.includes(option.value)}
													<option value={option.value}>{option.label}</option>
												{/if}
											{/each}
										</select>
									</div>
								{/if}
							</div>
						</div>

						<div class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}">
							<div class="flex items-center space-x-2">
								<input
									type="checkbox"
									id="no-dolby-video"
									bind:checked={noDolbyVideo}
									class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
								/>
								<Label for="no-dolby-video" class="text-sm">禁用杜比视界</Label>
							</div>

							<div class="flex items-center space-x-2">
								<input
									type="checkbox"
									id="no-dolby-audio"
									bind:checked={noDolbyAudio}
									class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
								/>
								<Label for="no-dolby-audio" class="text-sm">禁用杜比全景声</Label>
							</div>

							<div class="flex items-center space-x-2">
								<input
									type="checkbox"
									id="no-hdr"
									bind:checked={noHdr}
									class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
								/>
								<Label for="no-hdr" class="text-sm">禁用HDR</Label>
							</div>

							<div class="flex items-center space-x-2">
								<input
									type="checkbox"
									id="no-hires"
									bind:checked={noHires}
									class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
								/>
								<Label for="no-hires" class="text-sm">禁用Hi-Res音频</Label>
							</div>
						</div>
					</div>
					<SheetFooter class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}>
						<Button type="submit" disabled={saving} class="w-full">
							{saving ? '保存中...' : '保存设置'}
						</Button>
					</SheetFooter>
				</form>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- 下载设置抽屉 -->
<Sheet
	open={openSheet === 'download'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 2) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>下载设置</SheetTitle>
					<SheetDescription>并行下载、并发控制、速率限制配置</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							/>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						saveConfig();
					}}
					class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}"
				>
					<div class="flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}">
						<div class="mt-6 space-y-6">
							<h3 class="text-base font-semibold">下载配置</h3>

							<div class="flex items-center space-x-2">
								<input
									type="checkbox"
									id="parallel-download"
									bind:checked={parallelDownloadEnabled}
									class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
								/>
								<Label
									for="parallel-download"
									class="text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
								>
									启用多线程下载
								</Label>
							</div>

							{#if parallelDownloadEnabled}
								<div class="ml-6 space-y-2">
									<Label for="threads">下载线程数</Label>
									<Input
										id="threads"
										type="number"
										bind:value={parallelDownloadThreads}
										min="1"
										max="16"
										placeholder="4"
									/>
								</div>
							{/if}
						</div>

						<div class="mt-6 space-y-6">
							<h3 class="text-base font-semibold">并发控制</h3>

							<div class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}">
								<div class="space-y-2">
									<Label for="concurrent-video">同时处理视频数</Label>
									<Input
										id="concurrent-video"
										type="number"
										bind:value={concurrentVideo}
										min="1"
										max="10"
										placeholder="3"
									/>
								</div>

								<div class="space-y-2">
									<Label for="concurrent-page">每个视频并发分页数</Label>
									<Input
										id="concurrent-page"
										type="number"
										bind:value={concurrentPage}
										min="1"
										max="10"
										placeholder="2"
									/>
								</div>

								<div class="space-y-2">
									<Label for="rate-limit">请求频率限制</Label>
									<Input
										id="rate-limit"
										type="number"
										bind:value={rateLimit}
										min="1"
										max="100"
										placeholder="4"
									/>
									<p class="text-muted-foreground text-sm">每个时间窗口内的最大请求数</p>
								</div>

								<div class="space-y-2">
									<Label for="rate-duration">时间窗口（毫秒）</Label>
									<Input
										id="rate-duration"
										type="number"
										bind:value={rateDuration}
										min="100"
										max="5000"
										placeholder="250"
									/>
									<p class="text-muted-foreground text-sm">请求频率限制的时间窗口</p>
								</div>
							</div>
						</div>

						<div
							class="mt-6 rounded-lg border border-purple-200 bg-purple-50 p-3 dark:border-purple-800 dark:bg-purple-950/20"
						>
							<h5 class="mb-2 font-medium text-purple-800 dark:text-purple-200">并发控制说明</h5>
							<div class="space-y-1 text-sm text-purple-700 dark:text-purple-300">
								<p><strong>视频并发数：</strong>同时处理的视频数量（建议1-5）</p>
								<p><strong>分页并发数：</strong>每个视频内的并发分页数（建议1-3）</p>
								<p>
									<strong>请求频率限制：</strong>防止API请求过频繁导致风控，调小limit可减少被限制
								</p>
								<p><strong>总并行度：</strong>约等于 视频并发数 × 分页并发数</p>
							</div>
						</div>
					</div>
					<SheetFooter class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}>
						<Button type="submit" disabled={saving} class="w-full">
							{saving ? '保存中...' : '保存设置'}
						</Button>
					</SheetFooter>
				</form>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- 弹幕设置抽屉 -->
<Sheet
	open={openSheet === 'danmaku'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 3) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>弹幕设置</SheetTitle>
					<SheetDescription>弹幕显示样式和布局参数</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							/>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						saveConfig();
					}}
					class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}"
				>
					<div class="flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}">
						<div class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}">
							<div class="space-y-2">
								<Label for="danmaku-duration">弹幕持续时间（秒）</Label>
								<Input
									id="danmaku-duration"
									type="number"
									bind:value={danmakuDuration}
									min="1"
									max="60"
									step="0.1"
									placeholder="15.0"
								/>
							</div>

							<div class="space-y-2">
								<Label for="danmaku-font">字体</Label>
								<Input id="danmaku-font" bind:value={danmakuFont} placeholder="黑体" />
							</div>

							<div class="space-y-2">
								<Label for="danmaku-font-size">字体大小</Label>
								<Input
									id="danmaku-font-size"
									type="number"
									bind:value={danmakuFontSize}
									min="10"
									max="200"
									placeholder="25"
								/>
							</div>

							<div class="space-y-2">
								<Label for="danmaku-width-ratio">宽度比例</Label>
								<Input
									id="danmaku-width-ratio"
									type="number"
									bind:value={danmakuWidthRatio}
									min="0.1"
									max="3.0"
									step="0.1"
									placeholder="1.2"
								/>
							</div>

							<div class="space-y-2">
								<Label for="danmaku-horizontal-gap">水平间距</Label>
								<Input
									id="danmaku-horizontal-gap"
									type="number"
									bind:value={danmakuHorizontalGap}
									min="0"
									max="500"
									step="1"
									placeholder="20.0"
								/>
							</div>

							<div class="space-y-2">
								<Label for="danmaku-lane-size">轨道高度</Label>
								<Input
									id="danmaku-lane-size"
									type="number"
									bind:value={danmakuLaneSize}
									min="10"
									max="200"
									placeholder="32"
								/>
							</div>

							<div class="space-y-2">
								<Label for="danmaku-float-percentage">滚动弹幕占比</Label>
								<Input
									id="danmaku-float-percentage"
									type="number"
									bind:value={danmakuFloatPercentage}
									min="0"
									max="1"
									step="0.1"
									placeholder="0.5"
								/>
							</div>

							<div class="space-y-2">
								<Label for="danmaku-bottom-percentage">底部弹幕占比</Label>
								<Input
									id="danmaku-bottom-percentage"
									type="number"
									bind:value={danmakuBottomPercentage}
									min="0"
									max="1"
									step="0.1"
									placeholder="0.3"
								/>
							</div>

							<div class="space-y-2">
								<Label for="danmaku-opacity">不透明度（0-255）</Label>
								<Input
									id="danmaku-opacity"
									type="number"
									bind:value={danmakuOpacity}
									min="0"
									max="255"
									placeholder="76"
								/>
							</div>

							<div class="space-y-2">
								<Label for="danmaku-outline">描边宽度</Label>
								<Input
									id="danmaku-outline"
									type="number"
									bind:value={danmakuOutline}
									min="0"
									max="5"
									step="0.1"
									placeholder="0.8"
								/>
							</div>

							<div class="space-y-2">
								<Label for="danmaku-time-offset">时间偏移（秒）</Label>
								<Input
									id="danmaku-time-offset"
									type="number"
									bind:value={danmakuTimeOffset}
									step="0.1"
									placeholder="0.0"
								/>
							</div>

							<div class="flex items-center space-x-2">
								<input
									type="checkbox"
									id="danmaku-bold"
									bind:checked={danmakuBold}
									class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
								/>
								<Label for="danmaku-bold" class="text-sm">加粗字体</Label>
							</div>
						</div>

						<div
							class="rounded-lg border border-green-200 bg-green-50 p-3 dark:border-green-800 dark:bg-green-950/20"
						>
							<h5 class="mb-2 font-medium text-green-800 dark:text-green-200">弹幕设置说明</h5>
							<div class="space-y-1 text-sm text-green-700 dark:text-green-300">
								<p><strong>持续时间：</strong>弹幕在屏幕上显示的时间（秒）</p>
								<p><strong>字体样式：</strong>字体、大小、加粗、描边等外观设置</p>
								<p><strong>布局设置：</strong>轨道高度、间距、占比等位置控制</p>
								<p><strong>不透明度：</strong>0-255，0完全不透明，255完全透明</p>
								<p><strong>时间偏移：</strong>正值延后弹幕，负值提前弹幕</p>
							</div>
						</div>
					</div>
					<SheetFooter class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}>
						<Button type="submit" disabled={saving} class="w-full">
							{saving ? '保存中...' : '保存设置'}
						</Button>
					</SheetFooter>
				</form>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- B站凭证设置抽屉 -->
<Sheet
	open={openSheet === 'credential'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 4) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<div class="pr-8">
						<SheetTitle>B站凭证设置</SheetTitle>
						<SheetDescription>配置B站登录凭证信息</SheetDescription>
						{#if currentUser}
							<div
								class="mt-4 rounded-lg border border-green-200 bg-green-50 p-3 dark:border-green-800 dark:bg-green-950/20"
							>
								<div class="flex items-center space-x-3">
									<div class="bg-muted relative h-10 w-10 overflow-hidden rounded-full">
										{#if currentUser.avatar_url}
											<img
												src={getProxiedImageUrl(currentUser.avatar_url)}
												alt={currentUser.username}
												class="h-full w-full object-cover"
												loading="lazy"
											/>
										{:else}
											<div
												class="bg-muted flex h-full w-full items-center justify-center text-xs font-semibold"
											>
												{currentUser.username.slice(0, 2).toUpperCase()}
											</div>
										{/if}
									</div>
									<div class="flex-1">
										<div class="text-sm font-semibold text-green-800 dark:text-green-200">
											当前登录：{currentUser.username}
										</div>
										<div class="text-xs text-green-600 dark:text-green-400">
											UID: {currentUser.user_id}
										</div>
									</div>
									<Badge variant="default" class="bg-green-500">已登录</Badge>
								</div>
							</div>
						{/if}
					</div>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							/>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<div class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}">
					<Tabs.Root value="manual" class="flex-1">
						<Tabs.List
							class="grid w-full grid-cols-2 {isMobile ? 'mx-4' : 'mx-6'} mt-4"
							style="width: calc(100% - {isMobile ? '2rem' : '3rem'});"
						>
							<Tabs.Trigger value="manual">手动输入凭证</Tabs.Trigger>
							<Tabs.Trigger value="qr">扫码登录</Tabs.Trigger>
						</Tabs.List>

						<Tabs.Content value="manual" class="flex-1">
							<form
								onsubmit={(e) => {
									e.preventDefault();
									saveCredential();
								}}
								class="flex h-full flex-col"
							>
								<div
									class="flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}"
								>
									<div
										class="rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-800 dark:bg-amber-950/20"
									>
										<div class="space-y-2 text-sm text-amber-800 dark:text-amber-200">
											<div class="font-medium">🔐 如何获取B站登录凭证：</div>
											<ol class="ml-4 list-decimal space-y-1">
												<li>在浏览器中登录B站</li>
												<li>按F12打开开发者工具</li>
												<li>切换到"网络"(Network)标签</li>
												<li>刷新页面，找到任意一个请求</li>
												<li>在请求头中找到Cookie字段，复制对应的值</li>
											</ol>
											<div class="mt-2 text-xs text-amber-600 dark:text-amber-400">
												💡
												提示：SESSDATA、bili_jct、buvid3、DedeUserID是必填项，ac_time_value、buvid4、DedeUserID__ckMd5可选
											</div>
										</div>
									</div>

									<div
										class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}"
									>
										<div class="space-y-2">
											<Label for="sessdata">SESSDATA *</Label>
											<Input
												id="sessdata"
												type="password"
												bind:value={sessdata}
												placeholder="请输入SESSDATA"
											/>
										</div>

										<div class="space-y-2">
											<Label for="bili-jct">bili_jct *</Label>
											<Input
												id="bili-jct"
												type="password"
												bind:value={biliJct}
												placeholder="请输入bili_jct"
											/>
										</div>

										<div class="space-y-2">
											<Label for="buvid3">buvid3 *</Label>
											<Input id="buvid3" bind:value={buvid3} placeholder="请输入buvid3" />
										</div>

										<div class="space-y-2">
											<Label for="dedeuserid">DedeUserID *</Label>
											<Input
												id="dedeuserid"
												bind:value={dedeUserId}
												placeholder="请输入DedeUserID"
											/>
										</div>

										<div class="space-y-2 md:col-span-2">
											<Label for="ac-time-value">ac_time_value (可选)</Label>
											<Input
												id="ac-time-value"
												bind:value={acTimeValue}
												placeholder="请输入ac_time_value（可选）"
											/>
										</div>

										<div class="space-y-2">
											<Label for="buvid4">buvid4 (可选)</Label>
											<Input id="buvid4" bind:value={buvid4} placeholder="请输入buvid4（可选）" />
										</div>

										<div class="space-y-2">
											<Label for="dedeuserid-ckmd5">DedeUserID__ckMd5 (可选)</Label>
											<Input
												id="dedeuserid-ckmd5"
												bind:value={dedeUserIdCkMd5}
												placeholder="请输入DedeUserID__ckMd5（可选）"
											/>
										</div>
									</div>

									<div
										class="rounded-lg border border-green-200 bg-green-50 p-3 dark:border-green-800 dark:bg-green-950/20"
									>
										<div class="text-sm text-green-800 dark:text-green-200">
											<div class="mb-1 font-medium">✅ 凭证状态检查：</div>
											<div class="text-xs">
												{#if sessdata && biliJct && buvid3 && dedeUserId}
													<span class="text-green-600 dark:text-green-400"
														>✓ 必填凭证已填写完整</span
													>
												{:else}
													<span class="text-orange-600 dark:text-orange-400"
														>⚠ 请填写所有必填凭证项</span
													>
												{/if}
											</div>
										</div>
									</div>
								</div>
								<SheetFooter
									class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}
								>
									<Button type="submit" disabled={credentialSaving} class="w-full">
										{credentialSaving ? '保存中...' : '保存凭证'}
									</Button>
								</SheetFooter>
							</form>
						</Tabs.Content>

						<Tabs.Content value="qr" class="flex-1">
							<div class="flex h-full flex-col {isMobile ? 'px-4 py-4' : 'px-6 py-6'}">
								<div class="mx-auto w-full max-w-md">
									<QrLogin
										onLoginSuccess={handleQrLoginSuccess}
										onLoginError={handleQrLoginError}
										onLogout={handleLogout}
									/>
								</div>
							</div>
						</Tabs.Content>
					</Tabs.Root>
				</div>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- 风控配置抽屉 -->
<Sheet
	open={openSheet === 'risk'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 5) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>风控配置</SheetTitle>
					<SheetDescription>UP主投稿获取风控策略，用于优化大量视频UP主的获取</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							/>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						saveConfig();
					}}
					class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}"
				>
					<div class="flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}">
						<!-- 基础优化配置 -->
						<div
							class="rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-950/20"
						>
							<h3 class="mb-3 text-sm font-medium text-blue-800 dark:text-blue-200">
								🎯 基础优化配置
							</h3>
							<div class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}">
								<div class="space-y-2">
									<Label for="large-submission-threshold">大量视频UP主阈值</Label>
									<Input
										id="large-submission-threshold"
										type="number"
										bind:value={largeSubmissionThreshold}
										min="10"
										max="1000"
										placeholder="100"
									/>
									<p class="text-muted-foreground text-xs">超过此视频数量的UP主将启用风控策略</p>
								</div>

								<div class="space-y-2">
									<Label for="base-request-delay">基础请求间隔（毫秒）</Label>
									<Input
										id="base-request-delay"
										type="number"
										bind:value={baseRequestDelay}
										min="50"
										max="2000"
										placeholder="200"
									/>
									<p class="text-muted-foreground text-xs">每个请求之间的基础延迟时间</p>
								</div>

								<div class="space-y-2">
									<Label for="large-submission-delay-multiplier">大量视频延迟倍数</Label>
									<Input
										id="large-submission-delay-multiplier"
										type="number"
										bind:value={largeSubmissionDelayMultiplier}
										min="1"
										max="10"
										step="0.5"
										placeholder="2"
									/>
									<p class="text-muted-foreground text-xs">大量视频UP主的延迟倍数</p>
								</div>

								<div class="space-y-2">
									<Label for="max-delay-multiplier">最大延迟倍数</Label>
									<Input
										id="max-delay-multiplier"
										type="number"
										bind:value={maxDelayMultiplier}
										min="1"
										max="20"
										step="0.5"
										placeholder="4"
									/>
									<p class="text-muted-foreground text-xs">渐进式延迟的最大倍数限制</p>
								</div>
							</div>

							<div class="mt-4 flex items-center space-x-2">
								<input
									type="checkbox"
									id="enable-progressive-delay"
									bind:checked={enableProgressiveDelay}
									class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
								/>
								<Label for="enable-progressive-delay" class="text-sm">启用渐进式延迟</Label>
								<p class="text-muted-foreground ml-2 text-xs">随着请求次数增加逐步延长延迟时间</p>
							</div>
						</div>

						<!-- 增量获取配置 -->
						<div
							class="rounded-lg border border-green-200 bg-green-50 p-4 dark:border-green-800 dark:bg-green-950/20"
						>
							<h3 class="mb-3 text-sm font-medium text-green-800 dark:text-green-200">
								📈 增量获取配置
							</h3>
							<div class="space-y-4">
								<div class="flex items-center space-x-2">
									<input
										type="checkbox"
										id="enable-incremental-fetch"
										bind:checked={enableIncrementalFetch}
										class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
									/>
									<Label for="enable-incremental-fetch" class="text-sm">启用增量获取</Label>
									<p class="text-muted-foreground ml-2 text-xs">
										优先获取最新视频，减少不必要的请求
									</p>
								</div>

								<div class="flex items-center space-x-2">
									<input
										type="checkbox"
										id="incremental-fallback-to-full"
										bind:checked={incrementalFallbackToFull}
										class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
									/>
									<Label for="incremental-fallback-to-full" class="text-sm"
										>增量获取失败时回退到全量获取</Label
									>
									<p class="text-muted-foreground ml-2 text-xs">确保数据完整性</p>
								</div>
							</div>
						</div>

						<!-- 分批处理配置 -->
						<div
							class="rounded-lg border border-purple-200 bg-purple-50 p-4 dark:border-purple-800 dark:bg-purple-950/20"
						>
							<h3 class="mb-3 text-sm font-medium text-purple-800 dark:text-purple-200">
								📦 分批处理配置
							</h3>
							<div class="space-y-4">
								<div class="flex items-center space-x-2">
									<input
										type="checkbox"
										id="enable-batch-processing"
										bind:checked={enableBatchProcessing}
										class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
									/>
									<Label for="enable-batch-processing" class="text-sm">启用分批处理</Label>
									<p class="text-muted-foreground ml-2 text-xs">
										将大量请求分批处理，降低服务器压力
									</p>
								</div>

								{#if enableBatchProcessing}
									<div
										class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}"
									>
										<div class="space-y-2">
											<Label for="batch-size">分批大小（页数）</Label>
											<Input
												id="batch-size"
												type="number"
												bind:value={batchSize}
												min="1"
												max="20"
												placeholder="5"
											/>
											<p class="text-muted-foreground text-xs">每批处理的页数</p>
										</div>

										<div class="space-y-2">
											<Label for="batch-delay-seconds">批次间延迟（秒）</Label>
											<Input
												id="batch-delay-seconds"
												type="number"
												bind:value={batchDelaySeconds}
												min="1"
												max="60"
												placeholder="2"
											/>
											<p class="text-muted-foreground text-xs">每批之间的等待时间</p>
										</div>
									</div>
								{/if}
							</div>
						</div>

						<!-- 自动退避配置 -->
						<div
							class="rounded-lg border border-orange-200 bg-orange-50 p-4 dark:border-orange-800 dark:bg-orange-950/20"
						>
							<h3 class="mb-3 text-sm font-medium text-orange-800 dark:text-orange-200">
								🔄 自动退避配置
							</h3>
							<div class="space-y-4">
								<div class="flex items-center space-x-2">
									<input
										type="checkbox"
										id="enable-auto-backoff"
										bind:checked={enableAutoBackoff}
										class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
									/>
									<Label for="enable-auto-backoff" class="text-sm">启用自动退避</Label>
									<p class="text-muted-foreground ml-2 text-xs">遇到错误时自动增加延迟时间</p>
								</div>

								{#if enableAutoBackoff}
									<div
										class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}"
									>
										<div class="space-y-2">
											<Label for="auto-backoff-base-seconds">自动退避基础时间（秒）</Label>
											<Input
												id="auto-backoff-base-seconds"
												type="number"
												bind:value={autoBackoffBaseSeconds}
												min="1"
												max="300"
												placeholder="10"
											/>
											<p class="text-muted-foreground text-xs">遇到错误时的基础等待时间</p>
										</div>

										<div class="space-y-2">
											<Label for="auto-backoff-max-multiplier">自动退避最大倍数</Label>
											<Input
												id="auto-backoff-max-multiplier"
												type="number"
												bind:value={autoBackoffMaxMultiplier}
												min="1"
												max="20"
												placeholder="5"
											/>
											<p class="text-muted-foreground text-xs">退避时间的最大倍数限制</p>
										</div>
									</div>
								{/if}
							</div>
						</div>

						<!-- 视频源间延迟配置 -->
						<div
							class="rounded-lg border border-indigo-200 bg-indigo-50 p-4 dark:border-indigo-800 dark:bg-indigo-950/20"
						>
							<h3 class="mb-3 text-sm font-medium text-indigo-800 dark:text-indigo-200">
								⏱️ 视频源间延迟配置
							</h3>
							<div class="space-y-4">
								<div
									class="grid grid-cols-1 gap-4 {isMobile ? 'sm:grid-cols-1' : 'md:grid-cols-2'}"
								>
									<div class="space-y-2">
										<Label for="source-delay-seconds">通用视频源间延迟（秒）</Label>
										<Input
											id="source-delay-seconds"
											type="number"
											bind:value={sourceDelaySeconds}
											min="0"
											max="60"
											placeholder="2"
										/>
										<p class="text-muted-foreground text-xs">
											每个视频源之间的基础延迟时间（收藏夹、合集等）
										</p>
									</div>

									<div class="space-y-2">
										<Label for="submission-source-delay-seconds">UP主投稿源间延迟（秒）</Label>
										<Input
											id="submission-source-delay-seconds"
											type="number"
											bind:value={submissionSourceDelaySeconds}
											min="0"
											max="60"
											placeholder="5"
										/>
										<p class="text-muted-foreground text-xs">
											UP主投稿之间的特殊延迟时间（建议设置更长）
										</p>
									</div>
								</div>

								<div class="rounded-lg bg-indigo-100 p-3 dark:bg-indigo-900/20">
									<p class="text-sm text-indigo-700 dark:text-indigo-300">
										<strong>说明：</strong
										>在扫描多个视频源时，系统会在每个源之间自动添加延迟，避免连续请求触发风控。
										UP主投稿通常需要更长的延迟，因为其视频数量可能较多。设置为0可禁用延迟。
									</p>
								</div>
							</div>
						</div>

						<!-- 使用建议 -->
						<div
							class="rounded-lg border border-gray-200 bg-gray-50 p-4 dark:border-gray-700 dark:bg-gray-900/50"
						>
							<h3 class="mb-3 text-sm font-medium text-gray-800 dark:text-gray-200">💡 使用建议</h3>
							<div class="space-y-2 text-xs text-gray-600 dark:text-gray-400">
								<p><strong>小型UP主（&lt;100视频）：</strong> 使用默认设置即可</p>
								<p><strong>中型UP主（100-500视频）：</strong> 启用渐进式延迟和增量获取</p>
								<p><strong>大型UP主（500-1000视频）：</strong> 启用分批处理，设置较大的延迟倍数</p>
								<p>
									<strong>超大型UP主（&gt;1000视频）：</strong> 启用所有风控策略，适当增加各项延迟参数
								</p>
								<p><strong>频繁遇到412错误：</strong> 增加基础请求间隔和延迟倍数</p>
							</div>
						</div>
					</div>
					<SheetFooter class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}>
						<Button type="submit" disabled={saving} class="w-full">
							{saving ? '保存中...' : '保存设置'}
						</Button>
					</SheetFooter>
				</form>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- Aria2监控设置抽屉 -->
<Sheet
	open={openSheet === 'aria2'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 7) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>Aria2监控设置</SheetTitle>
					<SheetDescription>下载器健康检查和自动重启配置</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							/>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						saveConfig();
					}}
					class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}"
				>
					<div class="flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}">
						<!-- Aria2监控配置 -->
						<div
							class="rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-950/20"
						>
							<h3 class="mb-3 text-sm font-medium text-blue-800 dark:text-blue-200">
								🔍 健康检查配置
							</h3>
							<div class="space-y-4">
								<div class="flex items-center space-x-2">
									<input
										type="checkbox"
										id="enable-aria2-health-check"
										bind:checked={enableAria2HealthCheck}
										class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
									/>
									<Label for="enable-aria2-health-check" class="text-sm">启用Aria2健康检查</Label>
									<p class="text-muted-foreground ml-2 text-xs">定期检查下载器进程状态和RPC连接</p>
								</div>

								{#if enableAria2HealthCheck}
									<div class="ml-6 space-y-4">
										<div class="space-y-2">
											<Label for="aria2-health-check-interval">健康检查间隔（秒）</Label>
											<Input
												id="aria2-health-check-interval"
												type="number"
												bind:value={aria2HealthCheckInterval}
												min="30"
												max="600"
												placeholder="300"
											/>
											<p class="text-muted-foreground text-xs">
												检查频率，范围：30-600秒，推荐：300秒（5分钟）
											</p>
										</div>
									</div>
								{/if}
							</div>
						</div>

						<!-- 自动重启配置 -->
						<div
							class="rounded-lg border border-green-200 bg-green-50 p-4 dark:border-green-800 dark:bg-green-950/20"
						>
							<h3 class="mb-3 text-sm font-medium text-green-800 dark:text-green-200">
								🔄 自动重启配置
							</h3>
							<div class="space-y-4">
								<div class="flex items-center space-x-2">
									<input
										type="checkbox"
										id="enable-aria2-auto-restart"
										bind:checked={enableAria2AutoRestart}
										class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
									/>
									<Label for="enable-aria2-auto-restart" class="text-sm">启用自动重启</Label>
									<p class="text-muted-foreground ml-2 text-xs">检测到下载器异常时自动重启实例</p>
								</div>

								{#if !enableAria2AutoRestart}
									<div
										class="ml-6 rounded border border-orange-200 bg-orange-50 p-3 dark:border-orange-800 dark:bg-orange-950/20"
									>
										<p class="text-sm text-orange-700 dark:text-orange-300">
											<strong>注意：</strong
											>禁用自动重启后，检测到下载器异常时只会记录日志，不会自动恢复。
											如果下载器进程意外退出，需要手动重启应用程序。
										</p>
									</div>
								{/if}
							</div>
						</div>

						<!-- 配置说明 -->
						<div
							class="rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-800 dark:bg-amber-950/20"
						>
							<h3 class="mb-3 text-sm font-medium text-amber-800 dark:text-amber-200">
								⚠️ 重要说明
							</h3>
							<div class="space-y-2 text-sm text-amber-700 dark:text-amber-300">
								<p>
									<strong>为什么要禁用监控？</strong>
									原有的Aria2监控机制可能会误判下载器状态，导致不必要的重启，反而中断正在进行的下载任务。
								</p>
								<p>
									<strong>推荐配置：</strong>
								</p>
								<ul class="ml-4 list-disc space-y-1">
									<li><strong>稳定环境</strong>：建议禁用健康检查和自动重启</li>
									<li>
										<strong>不稳定环境</strong>：可启用健康检查，将间隔设为较长时间（5-10分钟）
									</li>
									<li><strong>测试环境</strong>：可启用全部功能进行调试</li>
								</ul>
								<p>
									<strong>注意事项：</strong> 修改这些设置需要重启应用程序才能生效。
								</p>
							</div>
						</div>

						<!-- 故障排除指南 -->
						<div
							class="rounded-lg border border-purple-200 bg-purple-50 p-4 dark:border-purple-800 dark:bg-purple-950/20"
						>
							<h3 class="mb-3 text-sm font-medium text-purple-800 dark:text-purple-200">
								🔧 故障排除
							</h3>
							<div class="space-y-2 text-sm text-purple-700 dark:text-purple-300">
								<p><strong>常见问题及解决方案：</strong></p>
								<ul class="ml-4 list-disc space-y-1">
									<li>
										<strong>下载频繁中断：</strong> 禁用健康检查，或增加检查间隔到600秒
									</li>
									<li>
										<strong>下载器启动失败：</strong> 检查系统防火墙和端口占用，禁用自动重启
									</li>
									<li>
										<strong>系统资源占用高：</strong> 增加健康检查间隔，减少监控频率
									</li>
									<li>
										<strong>下载任务丢失：</strong> 禁用自动重启，避免任务队列被重置
									</li>
								</ul>
							</div>
						</div>
					</div>
					<SheetFooter class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}>
						<Button type="submit" disabled={saving} class="w-full">
							{saving ? '保存中...' : '保存设置'}
						</Button>
					</SheetFooter>
				</form>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- 界面设置抽屉 -->
<Sheet
	open={openSheet === 'interface'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 7) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>界面设置</SheetTitle>
					<SheetDescription>主题模式、显示选项等界面配置</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							></path>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<div
					class="bg-background/50 {isMobile
						? 'flex-1 overflow-y-auto px-4 pb-4'
						: 'flex-1 overflow-y-auto p-6'}"
				>
					<div class="mx-auto max-w-4xl space-y-6">
						<!-- 主题设置 -->
						<div class="space-y-4">
							<div class="flex items-center justify-between">
								<div>
									<h3 class="text-lg font-medium">主题模式</h3>
									<p class="text-muted-foreground text-sm">选择您偏好的界面主题</p>
								</div>
								<div class="flex items-center gap-2">
									<span class="text-muted-foreground text-sm">快速切换:</span>
									<ThemeToggle />
								</div>
							</div>

							<div class="space-y-3">
								<h4 class="text-sm font-medium">快速切换</h4>
								<div class="grid grid-cols-3 gap-3">
									<button
										class="hover:bg-accent rounded-lg border p-3 text-center transition-colors {$theme ===
										'light'
											? 'border-primary bg-primary/10'
											: 'border-border'}"
										onclick={() => setTheme('light')}
									>
										<div class="bg-background mb-2 rounded-md border p-2">
											<div class="h-8 rounded bg-gradient-to-r from-gray-100 to-gray-200"></div>
										</div>
										<span class="text-xs font-medium">浅色模式</span>
									</button>

									<button
										class="hover:bg-accent rounded-lg border p-3 text-center transition-colors {$theme ===
										'dark'
											? 'border-primary bg-primary/10'
											: 'border-border'}"
										onclick={() => setTheme('dark')}
									>
										<div class="mb-2 rounded-md border bg-slate-900 p-2">
											<div class="h-8 rounded bg-gradient-to-r from-slate-700 to-slate-800"></div>
										</div>
										<span class="text-xs font-medium">深色模式</span>
									</button>

									<button
										class="hover:bg-accent rounded-lg border p-3 text-center transition-colors {$theme ===
										'system'
											? 'border-primary bg-primary/10'
											: 'border-border'}"
										onclick={() => setTheme('system')}
									>
										<div
											class="mb-2 rounded-md border bg-gradient-to-r from-gray-100 to-slate-900 p-2"
										>
											<div class="h-8 rounded bg-gradient-to-r from-gray-200 to-slate-700"></div>
										</div>
										<span class="text-xs font-medium">跟随系统</span>
									</button>
								</div>
							</div>

							<div
								class="rounded-lg border border-blue-200 bg-blue-50 p-3 dark:border-blue-800 dark:bg-blue-950/20"
							>
								<h5 class="mb-2 font-medium text-blue-800 dark:text-blue-200">主题说明</h5>
								<div class="space-y-1 text-sm text-blue-700 dark:text-blue-300">
									<p><strong>浅色模式：</strong>适合在明亮环境下使用，提供清晰的视觉体验</p>
									<p><strong>深色模式：</strong>适合在昏暗环境下使用，减少眼部疲劳</p>
									<p><strong>跟随系统：</strong>根据操作系统的主题设置自动切换</p>
								</div>
							</div>
						</div>
					</div>
				</div>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- 系统设置抽屉 -->
<Sheet
	open={openSheet === 'system'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 6) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>系统设置</SheetTitle>
					<SheetDescription>扫描间隔等其他设置</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							/>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						saveConfig();
					}}
					class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}"
				>
					<div class="flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}">
						<!-- Basic System Settings -->
						<div class="mt-6 space-y-6">
							<h3 class="text-base font-semibold">基本系统设置</h3>

							<div class="space-y-2">
								<Label for="interval">扫描间隔（秒）</Label>
								<Input
									id="interval"
									type="number"
									bind:value={interval}
									min="60"
									placeholder="1200"
								/>
								<p class="text-muted-foreground text-sm">每次扫描下载的时间间隔</p>
							</div>

							<div class="space-y-2">
								<Label for="bind-address">服务器端口</Label>
								<Input
									id="bind-address"
									type="text"
									bind:value={bindAddress}
									placeholder="0.0.0.0:12345"
									class={bindAddressValid ? '' : 'border-red-500'}
									on:input={() => validateBindAddress(bindAddress)}
								/>
								{#if bindAddressError}
									<p class="text-sm text-red-500">{bindAddressError}</p>
								{:else}
									<p class="text-muted-foreground text-sm">
										服务器监听地址和端口（修改后需要重启程序生效）
									</p>
								{/if}
							</div>

							<div class="flex items-center space-x-2">
								<input
									type="checkbox"
									id="cdn-sorting"
									bind:checked={cdnSorting}
									class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
								/>
								<Label for="cdn-sorting" class="text-sm">启用CDN排序</Label>
								<p class="text-muted-foreground ml-2 text-sm">优化下载节点选择</p>
							</div>

							<div class="space-y-2">
								<Label for="upper-path">UP主头像保存路径</Label>
								<Input
									id="upper-path"
									type="text"
									bind:value={upperPath}
									placeholder="config/upper_face"
								/>
								<p class="text-muted-foreground text-sm">UP主头像和person.nfo文件的保存目录路径</p>
							</div>

							<div
								class="rounded-lg border border-orange-200 bg-orange-50 p-3 dark:border-orange-800 dark:bg-orange-950/20"
							>
								<h5 class="mb-2 font-medium text-orange-800 dark:text-orange-200">其他设置说明</h5>
								<div class="space-y-1 text-sm text-orange-700 dark:text-orange-300">
									<p><strong>扫描间隔：</strong>每次扫描下载的时间间隔（秒）</p>
									<p>
										<strong>内存映射优化：</strong
										>已自动启用，使用SQLite内存映射技术优化数据库性能，无需手动配置
									</p>
									<p><strong>CDN排序：</strong>启用后优先使用质量更高的CDN，可能提升下载速度</p>
									<p>
										<strong>UP主头像路径：</strong
										>UP主头像和person.nfo文件的保存目录，用于媒体库显示
									</p>
								</div>
							</div>
						</div>
					</div>
					<SheetFooter class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}>
						<Button type="submit" disabled={saving} class="w-full">
							{saving ? '保存中...' : '保存设置'}
						</Button>
					</SheetFooter>
				</form>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- 推送通知设置抽屉片段 -->
<Sheet
	open={openSheet === 'notification'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 8) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>推送通知设置</SheetTitle>
					<SheetDescription>配置扫描完成推送通知</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							/>
						</svg>
						<span class="sr-only">关闭</span>
					</button>
				</SheetHeader>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						saveNotificationConfig();
					}}
					class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}"
				>
					<div class="flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}">
						<!-- 推送状态卡片 -->
						{#if notificationStatus}
							<div
								class="rounded-lg border {notificationStatus.configured
									? 'border-green-200 bg-green-50 dark:border-green-800 dark:bg-green-950/20'
									: 'border-amber-200 bg-amber-50 dark:border-amber-800 dark:bg-amber-950/20'} p-4"
							>
								<div class="flex items-center space-x-2">
									{#if notificationStatus.configured}
										<Badge variant="default" class="bg-green-500">已配置</Badge>
										<span class="text-sm text-green-700 dark:text-green-400"
											>Server酱已配置，可以接收推送通知</span
										>
									{:else}
										<Badge variant="secondary">未配置</Badge>
										<span class="text-sm text-amber-700 dark:text-amber-400"
											>请配置Server酱密钥以启用推送功能</span
										>
									{/if}
								</div>
							</div>
						{/if}

						<!-- 启用推送通知 -->
						<div class="space-y-4">
							<div class="flex items-center space-x-2">
								<input
									type="checkbox"
									id="notification-enabled"
									bind:checked={notificationEnabled}
									class="text-primary focus:ring-primary h-4 w-4 rounded border-gray-300"
								/>
								<Label
									for="notification-enabled"
									class="text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
								>
									启用扫描完成推送通知
								</Label>
							</div>
							<p class="text-muted-foreground text-sm">
								当扫描完成且有新视频时，通过Server酱发送推送通知到您的微信
							</p>
						</div>

						<!-- Server酱配置 -->
						<div class="space-y-4">
							<h3 class="text-base font-semibold">Server酱配置</h3>

							<div class="space-y-2">
								<Label for="serverchan-key">Server酱 SendKey</Label>
								<Input
									id="serverchan-key"
									type="password"
									bind:value={serverchanKey}
									placeholder={notificationStatus?.configured
										? '已配置（留空保持不变）'
										: '请输入Server酱密钥'}
								/>
								<p class="text-muted-foreground text-sm">
									从 <a
										href="https://sct.ftqq.com/"
										target="_blank"
										class="text-primary hover:underline">sct.ftqq.com</a
									> 获取您的SendKey
								</p>
							</div>

							<div class="space-y-2">
								<Label for="min-videos">最小视频数阈值</Label>
								<Input
									id="min-videos"
									type="number"
									bind:value={notificationMinVideos}
									min="1"
									max="100"
									placeholder="1"
								/>
								<p class="text-muted-foreground text-sm">
									只有新增视频数量达到此阈值时才会发送推送通知
								</p>
							</div>
						</div>

						<!-- 测试推送 -->
						{#if notificationStatus?.configured}
							<div
								class="rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-950/20"
							>
								<h4 class="mb-3 font-medium text-blue-800 dark:text-blue-400">测试推送</h4>
								<p class="mb-3 text-sm text-blue-700 dark:text-blue-300">
									发送一条测试消息到您的推送接收端，验证配置是否正确
								</p>
								<Button type="button" variant="outline" size="sm" onclick={testNotification}>
									发送测试推送
								</Button>
							</div>
						{/if}

						<!-- 使用说明 -->
						<div
							class="rounded-lg border border-gray-200 bg-gray-50 p-4 dark:border-gray-700 dark:bg-gray-900/50"
						>
							<h4 class="mb-3 font-medium text-gray-800 dark:text-gray-200">使用说明</h4>
							<ol
								class="list-inside list-decimal space-y-2 text-sm text-gray-600 dark:text-gray-400"
							>
								<li>
									访问 <a
										href="https://sct.ftqq.com/"
										target="_blank"
										class="text-primary hover:underline">Server酱官网</a
									> 注册账号
								</li>
								<li>登录后在"SendKey"页面获取您的密钥</li>
								<li>将密钥填入上方输入框并保存</li>
								<li>使用测试按钮验证推送是否正常</li>
								<li>扫描完成后，如果有新视频将自动推送到您的微信</li>
							</ol>
						</div>

						<!-- 推送内容示例 -->
						<div
							class="rounded-lg border border-purple-200 bg-purple-50 p-4 dark:border-purple-800 dark:bg-purple-950/20"
						>
							<h4 class="mb-3 font-medium text-purple-800 dark:text-purple-400">推送内容示例</h4>
							<div class="space-y-2 font-mono text-sm text-purple-700 dark:text-purple-300">
								<p><strong>标题：</strong>Bili Sync 扫描完成</p>
								<p><strong>内容：</strong></p>
								<div class="ml-4 space-y-1">
									<p>📊 扫描摘要</p>
									<p>- 扫描视频源: 5个</p>
									<p>- 新增视频: 12个</p>
									<p>- 扫描耗时: 3.5分钟</p>
									<p></p>
									<p>📹 新增视频详情</p>
									<p>🎬 收藏夹 - 我的收藏 (3个新视频)</p>
									<p>- 视频标题1 (BV1xx...)</p>
									<p>- 视频标题2 (BV1yy...)</p>
									<p>...</p>
								</div>
							</div>
						</div>
					</div>
					<SheetFooter class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}>
						<Button type="submit" disabled={notificationSaving} class="w-full">
							{notificationSaving ? '保存中...' : '保存设置'}
						</Button>
					</SheetFooter>
				</form>
			</div>
		</div>
	</SheetContent>
</Sheet>

<!-- 验证码风控设置抽屉 -->
<Sheet
	open={openSheet === 'captcha'}
	onOpenChange={(open) => {
		if (!open) openSheet = null;
	}}
>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[90vh] max-h-[90vh] w-full max-w-none overflow-hidden'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden"
	>
		{#if !isMobile && randomCovers.length > 0}
			<!-- 电脑端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={randomCovers[(currentBackgroundIndex + 8) % randomCovers.length]}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, {$isDark
						? 'rgba(0,0,0,0.85), rgba(0,0,0,0.5)'
						: 'rgba(255,255,255,0.85), rgba(255,255,255,0.5)'});"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader class="{isMobile ? 'border-b p-4' : 'border-b p-6'} relative">
					<SheetTitle>验证码风控设置</SheetTitle>
					<SheetDescription>v_voucher验证码风控配置，用于处理B站的风控验证</SheetDescription>
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => (openSheet = null)}
						class="ring-offset-background focus:ring-ring absolute top-2 right-2 rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none dark:hover:bg-gray-800"
						type="button"
						aria-label="关闭"
					>
						<svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M6 18L18 6M6 6l12 12"
							></path>
						</svg>
					</button>
				</SheetHeader>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						saveRiskControlConfig();
					}}
					class="flex flex-col {isMobile ? 'h-[calc(90vh-8rem)]' : 'h-[calc(100vh-12rem)]'}"
				>
					<div class="flex-1 space-y-6 overflow-y-auto {isMobile ? 'px-4 py-4' : 'px-6 py-6'}">
						<div class="space-y-4">
							<div class="space-y-2">
								<Label for="risk-control-enabled">启用风控验证</Label>
								<input
									id="risk-control-enabled"
									type="checkbox"
									bind:checked={riskControlEnabled}
									class="h-4 w-4"
								/>
								<p class="text-muted-foreground text-xs">
									启用后，遇到v_voucher风控时将进行验证码验证
								</p>
							</div>

							<div class="space-y-2">
								<Label for="risk-control-mode">验证模式</Label>
								<select
									id="risk-control-mode"
									bind:value={riskControlMode}
									class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
								>
									<option value="manual">manual - 手动验证</option>
									<option value="auto">auto - 自动验证</option>
									<option value="skip">skip - 跳过验证</option>
								</select>
								<p class="text-muted-foreground text-xs">
									manual: 弹出验证页面进行手动验证；auto: 使用第三方服务自动解决验证码；skip:
									直接跳过风控验证
								</p>
							</div>

							<div class="space-y-2">
								<Label for="risk-control-timeout">验证超时时间（秒）</Label>
								<Input
									id="risk-control-timeout"
									type="number"
									bind:value={riskControlTimeout}
									min="60"
									max="3600"
									placeholder="300"
								/>
								<p class="text-muted-foreground text-xs">
									用户完成验证码验证的最大等待时间，超时后将重新开始验证流程
								</p>
							</div>

							<!-- 自动验证配置 (仅在auto模式下显示) -->
							{#if riskControlMode === 'auto'}
								<div class="space-y-4 rounded-lg border bg-gray-50 p-4 dark:bg-gray-900/50">
									<h4 class="text-sm font-medium text-gray-900 dark:text-gray-100">自动验证配置</h4>

									<div class="space-y-2">
										<Label for="auto-solve-service">验证码服务</Label>
										<select
											id="auto-solve-service"
											bind:value={autoSolveService}
											class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
										>
											<option value="2captcha">2Captcha</option>
											<option value="anticaptcha">AntiCaptcha</option>
										</select>
										<p class="text-muted-foreground text-xs">选择验证码识别服务提供商</p>
									</div>

									<div class="space-y-2">
										<Label for="auto-solve-api-key">API密钥</Label>
										<Input
											id="auto-solve-api-key"
											type="password"
											bind:value={autoSolveApiKey}
											placeholder="输入API密钥"
										/>
										<p class="text-muted-foreground text-xs">
											验证码服务的API密钥，请确保账户有足够余额
										</p>
									</div>

									<div class="grid grid-cols-2 gap-4">
										<div class="space-y-2">
											<Label for="auto-solve-max-retries">最大重试次数</Label>
											<Input
												id="auto-solve-max-retries"
												type="number"
												bind:value={autoSolveMaxRetries}
												min="1"
												max="10"
												placeholder="3"
											/>
										</div>

										<div class="space-y-2">
											<Label for="auto-solve-timeout">识别超时（秒）</Label>
											<Input
												id="auto-solve-timeout"
												type="number"
												bind:value={autoSolveTimeout}
												min="60"
												max="600"
												placeholder="300"
											/>
										</div>
									</div>

									<div class="rounded-lg bg-yellow-100 p-3 dark:bg-yellow-900/20">
										<p class="text-sm text-yellow-700 dark:text-yellow-300">
											<strong>费用说明：</strong>
										</p>
										<div class="space-y-1 text-sm text-yellow-700 dark:text-yellow-300">
											<p>• 2Captcha: 约$2.99/1000次GeeTest验证</p>
											<p>• AntiCaptcha: 约$2.89/1000次GeeTest验证</p>
											<p>• 建议先小额充值测试服务稳定性</p>
											<p>• 识别失败不会扣费，但重试会产生费用</p>
										</div>
									</div>
								</div>
							{/if}

							<div class="rounded-lg bg-blue-100 p-3 dark:bg-blue-900/20">
								<p class="text-sm text-blue-700 dark:text-blue-300">
									<strong>验证流程说明：</strong>
								</p>
								<div class="space-y-1 text-sm text-blue-700 dark:text-blue-300">
									<p>1. 当遇到v_voucher风控时，程序会自动暂停下载</p>
									<p>2. <strong>手动模式：</strong>在管理页面的 /captcha 路径提供验证界面</p>
									<p>3. <strong>自动模式：</strong>自动调用第三方服务识别验证码</p>
									<p>4. 完成验证后，程序会自动继续下载流程</p>
									<p>5. 验证结果会缓存1小时，避免重复验证</p>
								</div>
							</div>

							<div class="rounded-lg bg-orange-100 p-3 dark:bg-orange-900/20">
								<p class="text-sm text-orange-700 dark:text-orange-300">
									<strong>注意事项：</strong>
								</p>
								<div class="space-y-1 text-sm text-orange-700 dark:text-orange-300">
									<p>• <strong>手动模式：</strong>验证码验证需要在浏览器中手动完成</p>
									<p>• <strong>自动模式：</strong>需要有效的API密钥和账户余额</p>
									<p>• 建议将验证超时时间设置为3-5分钟</p>
									<p>• 跳过验证可能导致部分视频无法下载</p>
									<p>• 自动验证失败时会自动回退到手动模式</p>
								</div>
							</div>
						</div>
					</div>
					<SheetFooter class={isMobile ? 'pb-safe border-t px-4 pt-3' : 'pb-safe border-t pt-4'}>
						<Button type="submit" disabled={isSaving} class="w-full">
							{isSaving ? '保存中...' : '保存设置'}
						</Button>
					</SheetFooter>
				</form>
			</div>
		</div>
	</SheetContent>
</Sheet>
