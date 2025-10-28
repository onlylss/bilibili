<script lang="ts">
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import type { ApiError, VideoInfo } from '$lib/types';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import InfoIcon from '@lucide/svelte/icons/info';
	import UserIcon from '@lucide/svelte/icons/user';
	import DownloadIcon from '@lucide/svelte/icons/download';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import { goto } from '$app/navigation';
	import api from '$lib/api';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import { toast } from 'svelte-sonner';

	export let video: VideoInfo;
	export let showActions: boolean = true; // 控制是否显示操作按钮
	export let mode: 'default' | 'detail' | 'page' = 'default'; // 卡片模式
	export let customTitle: string = ''; // 自定义标题
	export let customSubtitle: string = ''; // 自定义副标题
	export let taskNames: string[] = []; // 自定义任务名称
	export let showProgress: boolean = true; // 是否显示进度信息
	export let onReset: ((force: boolean) => Promise<void>) | null = null; // 自定义重置函数
	export let resetDialogOpen = false; // 导出对话框状态，让父组件可以控制
	export let resetting = false;
	export let selectionMode: boolean = false; // 是否为选择模式
	export let selected: boolean = false; // 是否被选中
	export let onSelectionChange: ((videoId: number, selected: boolean) => void) | null = null; // 选择状态变化回调

	let deleteDialogOpen = false; // 删除确认对话框状态
	let deleting = false; // 是否正在删除

	function getStatusText(status: number): string {
		if (status === 7) {
			return '已完成';
		} else if (status === 0) {
			return '未开始';
		} else {
			return `失败${status}次`;
		}
	}

	function getSegmentColor(status: number): string {
		if (status === 7) {
			return 'bg-green-500'; // 绿色 - 成功
		} else if (status === 0) {
			return 'bg-yellow-500'; // 黄色 - 未开始
		} else {
			return 'bg-red-500'; // 红色 - 失败
		}
	}

	function getOverallStatus(downloadStatus: number[], autoDownload: boolean, rawDownloadStatus: number): {
		text: string;
		color: 'default' | 'secondary' | 'destructive' | 'outline';
	} {
		// 检查是否为充电专享视频（第30位为1）
		const isChargingVideo = (rawDownloadStatus & (1 << 30)) !== 0;

		if (isChargingVideo) {
			return { text: '充电视频', color: 'destructive' };
		}

		const completed = downloadStatus.filter((status) => status === 7).length;
		const total = downloadStatus.length;
		const failed = downloadStatus.filter((status) => status !== 7 && status !== 0).length;

		if (completed === total) {
			return { text: '全部完成', color: 'default' };
		} else if (failed > 0) {
			return { text: '部分失败', color: 'destructive' };
		} else if (!autoDownload) {
			return { text: '未下载', color: 'outline' };
		} else {
			return { text: '进行中', color: 'secondary' };
		}
	}

	function getTaskName(index: number): string {
		if (taskNames.length > 0) {
			return taskNames[index] || `任务${index + 1}`;
		}

		// 根据视频类型返回不同的任务名称
		const isBangumi = video.bangumi_title !== undefined;

		if (isBangumi) {
			// 番剧任务名称：VideoStatus[2] 对应 tvshow.nfo 生成
			const bangumiTaskNames = ['视频封面', '视频信息', 'tvshow.nfo', 'UP主信息', '分P下载'];
			return bangumiTaskNames[index] || `任务${index + 1}`;
		} else {
			// 普通视频任务名称：VideoStatus[2] 对应 UP主头像下载
			const defaultTaskNames = ['视频封面', '视频信息', 'UP主头像', 'UP主信息', '分P下载'];
			return defaultTaskNames[index] || `任务${index + 1}`;
		}
	}

	// 过滤出实际执行的任务（隐藏被禁用的任务）
	// 根据提交 5f42566c8d25ea1b64367c3c385d689afd1d97e1，大部分任务已被禁用
	// VideoStatus: 只保留第4个任务（分P下载）
	// PageStatus: 只保留第1个（视频内容）和第3个（弹幕）任务
	function getActiveTasksWithStatus(downloadStatus: number[]): { index: number; status: number }[] {
		if (taskNames.length > 0) {
			// 自定义任务名称，显示所有任务
			return downloadStatus.map((status, index) => ({ index, status }));
		}

		// 对于标准的 VideoStatus（5个任务），只显示第4个任务（分P下载）
		// 其他任务（封面、信息、UP主头像、UP主信息）都已被禁用
		return [{ index: 4, status: downloadStatus[4] }];
	}

	// 只计算实际执行任务的进度
	$: activeTasks = getActiveTasksWithStatus(video.download_status);
	$: activeStatuses = activeTasks.map(t => t.status);
	$: overallStatus = getOverallStatus(activeStatuses, video.auto_download, video.raw_download_status);
	$: completed = activeStatuses.filter((status) => status === 7).length;
	$: total = activeStatuses.length;

	async function handleReset(force: boolean = false) {
		resetting = true;
		try {
			if (onReset) {
				await onReset(force);
			} else {
				const response = await api.resetVideo(video.id, force);
				// 根据返回结果显示不同的提示
				if (response.data.resetted) {
					toast.success('重置成功', {
						description: `已重置 ${response.data.pages.length} 个分P${force ? ' (强制重置)' : ''}`
					});

					// 重新获取最新的视频状态
					try {
						const videoResponse = await api.getVideo(video.id);
						if (videoResponse.data.video) {
							// 更新本地视频对象的下载状态
							video.download_status = videoResponse.data.video.download_status;
							video.auto_download = videoResponse.data.video.auto_download;
							video = video; // 触发 Svelte 响应式更新
						}
					} catch (err) {
						console.error('获取最新视频状态失败:', err);
					}
				} else {
					if (force) {
						toast.info('无任务可重置', {
							description: '该视频暂无任何任务'
						});
					} else {
						toast.info('重置无效', {
							description: '所有任务均成功，无需重置。如需重新下载，请使用强制重置。'
						});
					}
				}
			}
		} catch (error) {
			console.error('重置失败:', error);
			toast.error('重置失败', {
				description: (error as ApiError).message
			});
		} finally {
			resetting = false;
			resetDialogOpen = false;
		}
	}

	function handleViewDetail() {
		goto(`/video/${video.id}`);
	}

	function handleSelectionChange(event: Event) {
		if (onSelectionChange) {
			const checkbox = event.target as HTMLInputElement;
			onSelectionChange(video.id, checkbox.checked);
		}
	}

	// 处理标记为自动下载
	let downloadingVideo = false;
	async function handleMarkForDownload() {
		downloadingVideo = true;
		try {
			const response = await api.updateVideoAutoDownload(video.id, true);
			if (response.data.success) {
				toast.success('标记成功', {
					description: response.data.message
				});
				// 更新本地状态，触发响应式更新
				video.auto_download = true;
				video = video; // 触发 Svelte 响应式更新
			}
		} catch (error) {
			console.error('标记下载失败:', error);
			toast.error('标记下载失败', {
				description: (error as ApiError).message
			});
		} finally {
			downloadingVideo = false;
		}
	}

	// 处理删除视频
	async function handleDelete() {
		deleting = true;
		try {
			const response = await api.deleteVideo(video.id);
			if (response.data.success) {
				toast.success('删除成功', {
					description: '视频已删除，自动下载已改为手动，进度已重置'
				});
				// 刷新页面或通知父组件
				window.location.reload();
			}
		} catch (error) {
			console.error('删除视频失败:', error);
			toast.error('删除失败', {
				description: (error as ApiError).message
			});
		} finally {
			deleting = false;
			deleteDialogOpen = false;
		}
	}

	// 根据模式确定显示的标题和副标题
	$: displayTitle = customTitle || getEnhancedVideoTitle(video);
	$: displaySubtitle = customSubtitle || video.upper_name;
	$: showUserIcon = mode === 'default';
	$: cardClasses =
		mode === 'default'
			? `group flex h-full min-w-0 flex-col transition-all hover:shadow-md ${selected ? 'ring-2 ring-blue-500' : ''}`
			: `transition-all hover:shadow-md ${selected ? 'ring-2 ring-blue-500' : ''}`;

	// 从路径中提取番剧名称的通用函数
	function extractBangumiName(path: string): string {
		if (!path) return '';
		const pathParts = path.split(/[/\\]/);
		// 查找最后一个非空的路径部分作为番剧名称
		for (let i = pathParts.length - 1; i >= 0; i--) {
			const part = pathParts[i].trim();
			if (part && part !== '.' && part !== '..') {
				return part;
			}
		}
		return '';
	}

	// 简化的番剧检测逻辑 - 直接使用category字段
	function isBangumiVideo(video: VideoInfo): boolean {
		return video.category === 1;
	}

	// 获取番剧名称用于显示
	function getBangumiName(video: VideoInfo): string {
		if (isBangumiVideo(video)) {
			// 优先使用API获取的真实番剧标题
			if (video.bangumi_title) {
				return video.bangumi_title;
			}
			// 回退到从路径提取
			return extractBangumiName(video.path);
		}
		return '';
	}

	// 获取集数信息用于显示 - 统一处理
	function getEpisodeInfo(video: VideoInfo): string {
		const originalName = video.name.trim();

		// 如果是番剧，尝试美化集数显示
		if (isBangumiVideo(video)) {
			// 如果是纯数字，加上"第X集"
			if (/^\d+$/.test(originalName)) {
				return `第${originalName}集`;
			}
			// 如果已经有"第X话"格式，保持原样
			if (/^第\d+[话集]/.test(originalName)) {
				return originalName;
			}
			// 其他情况直接返回原名
			return originalName;
		}

		return originalName;
	}

	// 统一的视频标题显示逻辑
	function getEnhancedVideoTitle(video: VideoInfo): string {
		// 如果检测到番剧，统一使用两行显示的第二行内容
		if (isBangumiVideo(video)) {
			return getEpisodeInfo(video);
		}

		// 非番剧直接返回原标题
		return video.name.trim();
	}

	// 获取代理后的图片URL
	function getProxiedImageUrl(originalUrl: string): string {
		if (!originalUrl) return '';
		// 使用后端代理端点
		return `/api/proxy/image?url=${encodeURIComponent(originalUrl)}`;
	}

	// 格式化发布日期为相对时间或具体日期
	function formatPubtime(pubtime: string): string {
		try {
			const date = new Date(pubtime.replace(' ', 'T'));
			const now = new Date();
			const diffMs = now.getTime() - date.getTime();
			const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

			// 只有今天发布的才显示相对时间
			if (diffDays === 0) {
				const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
				if (diffHours === 0) {
					const diffMinutes = Math.floor(diffMs / (1000 * 60));
					return diffMinutes <= 1 ? '刚刚' : `${diffMinutes}分钟前`;
				}
				return `${diffHours}小时前`;
			}

			// 不是今天发布的，显示具体日期
			const year = date.getFullYear();
			const month = String(date.getMonth() + 1).padStart(2, '0');
			const day = String(date.getDate()).padStart(2, '0');

			// 判断是否是今年
			if (year === now.getFullYear()) {
				return `${month}-${day}`;
			} else {
				return `${year}-${month}-${day}`;
			}
		} catch (e) {
			return pubtime;
		}
	}
</script>

<Card class="{cardClasses} relative overflow-hidden p-0 hover:shadow-lg transition-shadow cursor-pointer" onclick={handleViewDetail}>
	<!-- 封面图片 -->
	{#if video.cover && mode === 'default'}
		<div class="relative overflow-hidden bg-black">
			<!-- 封面图片 -->
			<img
				src={getProxiedImageUrl(video.cover)}
				alt={displayTitle}
				class="aspect-video w-full object-cover transition-transform duration-200 group-hover:scale-105 block"
				loading="lazy"
				onerror={(e) => {
					const target = e.currentTarget as HTMLImageElement;
					const container = target.closest('.relative') as HTMLElement;
					if (container) {
						container.style.display = 'none';
					}
				}}
			/>

			<!-- 遮罩层 - 底部渐变 -->
			<div class="absolute inset-x-0 bottom-0 h-16 bg-gradient-to-t from-black/60 to-transparent"></div>

			<!-- 选择模式复选框 -->
			{#if selectionMode}
				<div class="absolute top-1.5 left-1.5 z-20" onclick={(e) => e.stopPropagation()}>
					<input
						type="checkbox"
						checked={selected}
						onchange={handleSelectionChange}
						class="h-4 w-4 rounded border-2 border-white bg-white/90 text-blue-600 shadow-lg focus:ring-2 focus:ring-blue-500 focus:ring-offset-0"
					/>
				</div>
			{/if}

			<!-- 右上角状态标签 -->
			{#if overallStatus.text !== '全部完成'}
				<div class="absolute top-1.5 right-1.5 z-20">
					<Badge variant={overallStatus.color} class="text-[10px] px-1.5 py-0.5 shadow-md">
						{overallStatus.text}
					</Badge>
				</div>
			{/if}

			<!-- 左下角进度信息（B站风格） -->
			<div class="absolute bottom-1.5 left-1.5 z-20 flex items-center gap-2 text-white text-[11px] font-medium">
				<!-- 下载进度信息 -->
				<span class="flex items-center gap-0.5 bg-black/75 px-1.5 py-0.5 rounded">
					<svg class="h-3 w-3" fill="currentColor" viewBox="0 0 24 24">
						<path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/>
					</svg>
					{completed}/{total}
				</span>
			</div>
		</div>
	{/if}

	<!-- 标题和UP主信息区域（B站风格） -->
	<div class="p-2 space-y-1">
		<!-- 标题 -->
		<div class="line-clamp-2 text-sm leading-tight text-foreground" title={displayTitle}>
			{#if getBangumiName(video)}
				{getBangumiName(video)} · {getEpisodeInfo(video)}
			{:else}
				{displayTitle}
			{/if}
		</div>

		<!-- UP主信息 -->
		<div class="flex items-center gap-1 text-[11px] text-muted-foreground">
			<UserIcon class="h-3 w-3 shrink-0 opacity-70" />
			<span class="truncate">{displaySubtitle || video.upper_name}</span>
			<span class="shrink-0 opacity-60 mx-1">·</span>
			<span class="shrink-0 opacity-70">{formatPubtime(video.pubtime)}</span>
		</div>

		<!-- 操作按钮组 - 居中显示 -->
		{#if !selectionMode}
			<div class="flex justify-center gap-1 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 transition-opacity" onclick={(e) => e.stopPropagation()}>
				{#if !video.auto_download}
					<Button
						size="sm"
						variant="ghost"
						class="h-6 w-6 p-0"
						onclick={handleMarkForDownload}
						disabled={downloadingVideo}
						title="标记下载"
					>
						<DownloadIcon class="h-3 w-3" />
					</Button>
				{/if}
				<Button
					size="sm"
					variant="ghost"
					class="h-6 w-6 p-0"
					onclick={() => (resetDialogOpen = true)}
					title="重置"
				>
					<RotateCcwIcon class="h-3 w-3" />
				</Button>
				<Button
					size="sm"
					variant="ghost"
					class="h-6 w-6 p-0"
					onclick={() => (deleteDialogOpen = true)}
					title="删除"
				>
					<Trash2Icon class="h-3 w-3" />
				</Button>
			</div>
		{/if}
	</div>

	<!-- 隐藏的内容区域（保留原有功能） -->
	<CardContent class="hidden">
		<div class="space-y-1.5">

			<!-- 路径信息 -->
			{#if video.path && mode === 'detail'}
				<div class="mt-2 space-y-1">
					<div class="text-muted-foreground text-xs">保存路径</div>
					<div class="bg-muted rounded px-2 py-1 font-mono text-xs break-all" title={video.path}>
						{video.path}
					</div>
				</div>
			{/if}
		</div>
	</CardContent>
</Card>

<!-- 重置确认对话框 -->
<AlertDialog.Root bind:open={resetDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>确认重置</AlertDialog.Title>
			<AlertDialog.Description>
				<p class="mb-2">
					确定要重置视频 "{displayTitle}" 的下载状态吗？
				</p>
				<p class="text-muted-foreground text-sm">
					• <strong>重置失败</strong>：仅重置失败的任务<br />
					• <strong>强制重置</strong>：重置所有任务，重新下载
				</p>
			</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>取消</AlertDialog.Cancel>
			<Button variant="secondary" onclick={() => handleReset(false)} disabled={resetting}>
				{resetting ? '重置中...' : '重置失败'}
			</Button>
			<Button variant="destructive" onclick={() => handleReset(true)} disabled={resetting}>
				{resetting ? '重置中...' : '强制重置'}
			</Button>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

<!-- 删除确认对话框 -->
<AlertDialog.Root bind:open={deleteDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>确认删除</AlertDialog.Title>
			<AlertDialog.Description>
				<p class="mb-2">
					确定要删除视频 "{displayTitle}" 吗？
				</p>
				<p class="text-muted-foreground text-sm">
					此操作将执行以下操作：<br />
					• 删除视频文件<br />
					• 自动下载改为手动<br />
					• 下载进度重置<br />
					• 视频将显示在"未下载"分类中
				</p>
			</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>取消</AlertDialog.Cancel>
			<Button variant="destructive" onclick={handleDelete} disabled={deleting}>
				{deleting ? '删除中...' : '确认删除'}
			</Button>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
