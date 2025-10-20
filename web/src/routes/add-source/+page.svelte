<script lang="ts">
	import { goto } from '$app/navigation';
	import api from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import type {
		SearchResultItem,
		VideoCategory,
		SubmissionVideoInfo,
		UserFavoriteFolder,
		UserCollectionItem,
		UserFollowing,
		ValidateFavoriteResponse,
		UserCollectionInfo,
		AddVideoSourceRequest
	} from '$lib/types';
	import { Search, X, Plus as PlusIcon } from '@lucide/svelte';
	import { onDestroy, onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { flip } from 'svelte/animate';
	import { fade, fly } from 'svelte/transition';

	let sourceType: VideoCategory = 'collection';
	let lastSourceType: VideoCategory = sourceType; // 记录上一次的源类型，用于检测切换
	let sourceId = '';
	let upId = '';
	let name = '';
	let path = '';
	let cover = '';
	let collectionType = 'season';
	let downloadAllSeasons = false;
	let loading = false;

	// 添加手动输入标志
	let isManualInput = false;

	// 搜索相关
	let searchKeyword = '';
	let searchLoading = false;
	let searchResults: SearchResultItem[] = [];
	let showSearchResults = false;

	let searchTotalResults = 0;

	// 收藏夹相关
	let userFavorites: UserFavoriteFolder[] = [];
	let loadingFavorites = false;
	let validatingFavorite = false;
	let favoriteValidationResult: ValidateFavoriteResponse | null = null;
	let favoriteValidationTimeout: ReturnType<typeof setTimeout> | null = null;

	// UP主收藏夹搜索相关
	let searchedUserFavorites: UserFavoriteFolder[] = [];
	let loadingSearchedUserFavorites = false;
	let selectedUserId: string = '';
	let selectedUserName: string = '';

	// UP主合集相关
	let userCollections: UserCollectionItem[] = [];
	// eslint-disable-next-line @typescript-eslint/no-unused-vars
	let loadingCollections = false; // 合集加载状态
	let upIdTimeout: ReturnType<typeof setTimeout> | null = null;

	// 关注的UP主相关
	let userFollowings: UserFollowing[] = [];
	let loadingFollowings = false;

	// 过滤已有视频源相关
	let existingVideoSources: VideoSourcesResponse | null = null;
	let existingCollectionIds: Set<string> = new Set();
	let existingFavoriteIds: Set<number> = new Set();
	let existingSubmissionIds: Set<number> = new Set();
	let loadingExistingSources = false;

	// 批量添加相关
	let batchMode = false; // 是否为批量模式
	let batchSelectedItems = new Map(); // 存储选中项 {key: {type, data, name}}
	let batchCheckboxStates = {}; // 存储checkbox状态的响应式对象
	let batchBasePath = '/Downloads'; // 批量基础路径
	let batchAdding = false; // 批量添加进行中
	let batchProgress = { current: 0, total: 0 }; // 批量添加进度
	let batchDialogOpen = false; // 批量配置对话框状态

	// 响应式语句：当Map变化时更新checkbox状态对象
	$: {
		const newStates = {};
		for (const [key] of batchSelectedItems) {
			newStates[key] = true;
		}
		batchCheckboxStates = newStates;
		console.log('🔄 Reactive update - checkbox states:', Object.keys(newStates));
	}

	// 悬停详情相关
	let hoveredItem: {
		type: 'search';
		data: SearchResultItem;
	} | null = null;
	let hoverTimeout: ReturnType<typeof setTimeout> | null = null;
	let mousePosition = { x: 0, y: 0 };

	// 响应式相关
	let innerWidth: number;
	let isMobile: boolean = false;
	$: isMobile = innerWidth < 768; // md断点

	// 源类型选项
	const sourceTypeOptions = [
		{ value: 'collection', label: '合集', description: '视频合集，需要UP主ID和合集ID' },
		{
			value: 'favorite',
			label: '收藏夹',
			description: '可添加任何公开收藏夹，收藏夹ID可在收藏夹页面URL中获取'
		},
		{ value: 'submission', label: 'UP主投稿', description: 'UP主ID可在UP主空间URL中获取' },
		{ value: 'watch_later', label: '稍后观看', description: '同步稍后观看列表' }
	];

	// 合集类型选项
	const collectionTypeOptions = [
		{ value: 'season', label: '合集', description: 'B站标准合集' },
		{ value: 'series', label: '系列', description: '视频系列' }
	];

	// 订阅的合集相关
	let subscribedCollections: UserCollectionInfo[] = [];
	let loadingSubscribedCollections = false;

	// UP主投稿选择相关
	let showSubmissionSelection = false;
	let selectedVideos: string[] = [];
	// eslint-disable-next-line @typescript-eslint/no-unused-vars
	let selectedUpName = ''; // UP主名称，用于投稿选择

	// 投稿选择详细状态
	let submissionVideos: SubmissionVideoInfo[] = [];
	let selectedSubmissionVideos: Set<string> = new Set();
	let submissionLoading = false;
	let submissionError: string | null = null;
	let submissionTotalCount = 0;
	let submissionSearchQuery = '';
	let filteredSubmissionVideos: SubmissionVideoInfo[] = [];

	// 分页加载相关状态
	let currentLoadedPage = 0; // 当前加载到的页码
	let isLoadingMore = false; // 正在加载更多
	let hasMoreVideos = true; // 是否还有更多视频
	// eslint-disable-next-line @typescript-eslint/no-unused-vars
	let loadingProgress = ''; // 加载进度提示
	let showLoadMoreButton = false; // 是否显示加载更多按钮

	const SUBMISSION_PAGE_SIZE = 20;
	const INITIAL_LOAD_SIZE = 100; // 初始加载100个视频
	const LOAD_MORE_SIZE = 200; // 每次加载更多200个视频
	const PAGE_DELAY = 500; // 页面间延迟500ms

	// 滚动容器引用
	let submissionScrollContainer: HTMLElement;

	onMount(async () => {
		setBreadcrumb([
			{ label: '主页', href: '/' },
			{ label: '添加视频源', isActive: true }
		]);
		await loadExistingVideoSources();
	});

	onDestroy(() => {
		// 清理定时器
		clearTimeout(hoverTimeout);
		clearTimeout(upIdTimeout);
		clearTimeout(seasonIdTimeout);
		clearTimeout(favoriteValidationTimeout);
	});


	// 搜索B站内容
	async function handleSearch(overrideSearchType?: string) {
		if (!searchKeyword.trim()) {
			toast.error('请输入搜索关键词');
			return;
		}

		// 根据参数或当前选择的视频源类型确定搜索类型
		let searchType: 'video' | 'bili_user';
		if (overrideSearchType) {
			searchType = overrideSearchType as 'video' | 'bili_user';
		} else {
			switch (sourceType) {
				case 'collection':
				case 'submission':
				case 'favorite': // 收藏夹类型也搜索UP主
					searchType = 'bili_user';
					break;
				default:
					searchType = 'video';
					break;
			}
		}

		searchLoading = true;
		searchResults = [];
		searchTotalResults = 0;

		try {
			const pageSize = 50;

			// 第一次请求获取总数
			const firstResult = await api.searchBilibili({
				keyword: searchKeyword,
				search_type: searchType,
				page: 1,
				page_size: pageSize
			});

			if (!firstResult.data.success) {
				toast.error('搜索失败');
				return;
			}

			const totalResults = firstResult.data.total;
			searchTotalResults = totalResults;
			let allResults = [...firstResult.data.results];

			// 如果总数超过pageSize，继续获取剩余页面
			if (totalResults > pageSize) {
				const totalPages = Math.ceil(totalResults / pageSize);
				const remainingPages = Array.from({ length: totalPages - 1 }, (_, i) => i + 2);

				// 串行获取剩余页面，避免并发请求过多导致失败
				for (const page of remainingPages) {
					try {
						const pageResult = await api.searchBilibili({
							keyword: searchKeyword,
							search_type: searchType,
							page,
							page_size: pageSize
						});

						if (pageResult.data.success && pageResult.data.results) {
							allResults.push(...pageResult.data.results);
						}

						// 添加小延迟避免请求过于频繁
						await new Promise((resolve) => setTimeout(resolve, 100));
					} catch {
						// 静默处理失败，继续获取下一页
					}
				}
			}

			// 去重处理（基于season_id, bvid, mid等唯一标识）
			const uniqueResults = allResults.filter((result, index, arr) => {
				const id = result.season_id || result.bvid || result.mid || `${result.title}_${index}`;
				return (
					arr.findIndex((r) => {
						const rid = r.season_id || r.bvid || r.mid || `${r.title}_${arr.indexOf(r)}`;
						return rid === id;
					}) === index
				);
			});

			searchResults = uniqueResults;
			showSearchResults = true;

			// 优化提示信息
			const successRate = ((uniqueResults.length / totalResults) * 100).toFixed(1);
			if (uniqueResults.length < totalResults) {
				toast.success(
					`搜索完成，获取到 ${uniqueResults.length}/${totalResults} 个结果 (${successRate}%)`
				);
			} else {
				toast.success(`搜索完成，共获取到 ${uniqueResults.length} 个结果`);
			}
		} catch (error: unknown) {
			console.error('搜索失败:', error);
			const errorMessage = error instanceof Error ? error.message : '搜索失败';
			toast.error('搜索失败', { description: errorMessage });
		} finally {
			searchLoading = false;
		}
	}

	// 选择搜索结果
	function selectSearchResult(result: SearchResultItem) {
		switch (sourceType) {
			case 'collection':
				if (result.mid) {
					upId = result.mid.toString();
					// 触发获取UP主合集列表
					handleUpIdChange();
					toast.success('已填充UP主信息', { description: '正在获取合集列表...' });
				}
				break;
			case 'submission':
				if (result.mid) {
					sourceId = result.mid.toString();
					name = cleanTitle(result.title);
					selectedUpName = cleanTitle(result.title);
					// 打开投稿选择对话框
					showSubmissionSelection = true;
				}
				break;
			case 'favorite':
				// 收藏夹类型搜索UP主，调用获取收藏夹函数
				if (result.mid) {
					selectUserAndFetchFavorites(result);
					return; // 直接返回，不执行后续逻辑
				}
				break;
			default:
				if (result.bvid) {
					sourceId = result.bvid;
					name = cleanTitle(result.title);
				}
				break;
		}

		// 关闭搜索结果
		showSearchResults = false;
		searchResults = [];
		searchKeyword = '';
		searchTotalResults = 0;

		// 清除悬停状态
		hoveredItem = null;

		if (sourceType !== 'collection') {
			toast.success('已填充信息', { description: '请检查并完善其他必要信息' });
		}
	}

	// 清理标题中的HTML标签
	function cleanTitle(title: string): string {
		const div = document.createElement('div');
		div.innerHTML = title;
		return div.textContent || div.innerText || title;
	}

	// 处理图片URL
	function processBilibiliImageUrl(url: string): string {
		if (!url) return '';

		if (url.startsWith('https://')) return url;
		if (url.startsWith('//')) return 'https:' + url;
		if (url.startsWith('http://')) return url.replace('http://', 'https://');
		if (!url.startsWith('http')) return 'https://' + url;

		return url.split('@')[0];
	}

	// 处理图片加载错误
	function handleImageError(event: Event) {
		const img = event.target as HTMLImageElement;
		// 使用默认的占位图片
		img.style.display = 'none';
		const parent = img.parentElement;
		if (parent && !parent.querySelector('.placeholder')) {
			const placeholder = document.createElement('div');
			// 获取原图片的尺寸类
			const widthClass = img.className.match(/w-\d+/)?.[0] || 'w-20';
			const heightClass = img.className.match(/h-\d+/)?.[0] || 'h-14';
			placeholder.className = `placeholder ${widthClass} ${heightClass} bg-muted rounded flex items-center justify-center text-xs text-muted-foreground`;
			placeholder.textContent = '无图片';
			parent.appendChild(placeholder);
		}
	}

	async function handleSubmit() {
		// 验证表单
		if (sourceType !== 'watch_later' && !sourceId) {
			toast.error('请输入ID', { description: '视频源ID不能为空' });
			return;
		}

		if (sourceType === 'collection' && !upId) {
			toast.error('请输入UP主ID', { description: '合集需要提供UP主ID' });
			return;
		}

		if (!name) {
			toast.error('请输入名称', { description: '视频源名称不能为空' });
			return;
		}

		if (!path) {
			toast.error('请输入保存路径', { description: '保存路径不能为空' });
			return;
		}

		loading = true;

		try {
			const params: AddVideoSourceRequest = {
				source_type: sourceType,
				source_id: sourceId,
				name,
				path
			};

			if (sourceType === 'collection') {
				params.up_id = upId;
				params.collection_type = collectionType;
				if (cover) {
					params.cover = cover;
				}
			}


			if (sourceType === 'submission') {
				// 如果有选择的视频，添加selected_videos参数
				if (selectedVideos.length > 0) {
					params.selected_videos = selectedVideos;
				}
			}

			const result = await api.addVideoSource(params);

			if (result.data.success) {
				toast.success('添加成功', { description: result.data.message });
				// 重置表单
				sourceId = '';
				upId = '';
				name = '';
				path = '/Downloads';
				collectionType = 'season';
				isManualInput = false;
				selectedVideos = [];
				selectedUpName = '';
				// 跳转到视频源管理页面
				goto('/video-sources');
			} else {
				toast.error('添加失败', { description: result.data.message });
			}
		} catch (error: unknown) {
			console.error('添加视频源失败:', error);

			// 解析错误信息，提供更友好的提示
			let errorMessage = error instanceof Error ? error.message : '添加视频源失败';
			let errorDescription = '';

			if (errorMessage.includes('已存在')) {
				// 重复添加错误
				if (sourceType === 'collection') {
					errorDescription = '该合集已经添加过了，请检查是否使用了相同的合集ID和UP主ID';
				} else if (sourceType === 'favorite') {
					errorDescription = '该收藏夹已经添加过了，请检查是否使用了相同的收藏夹ID';
				} else if (sourceType === 'submission') {
					errorDescription = '该UP主的投稿已经添加过了，请检查是否使用了相同的UP主ID';
				} else if (sourceType === 'watch_later') {
					errorDescription = '稍后观看只能配置一个，请先删除现有配置';
				}

				toast.error('重复添加', {
					description: errorDescription,
					duration: 5000 // 延长显示时间
				});
			} else {
				// 其他错误
				toast.error('添加失败', { description: errorMessage });
			}
		} finally {
			loading = false;
		}
	}

	// 根据类型显示不同的描述
	$: currentTypeDescription =
		sourceTypeOptions.find((opt) => opt.value === sourceType)?.description || '';

	// 获取收藏夹列表
	async function fetchUserFavorites() {
		loadingFavorites = true;
		try {
			const result = await api.getUserFavorites();
			if (result.data) {
				userFavorites = result.data;
				toast.success('获取收藏夹成功', {
					description: `共获取到 ${userFavorites.length} 个收藏夹`
				});
			} else {
				toast.error('获取收藏夹失败');
			}
		} catch (error: unknown) {
			console.error('获取收藏夹失败:', error);
			const errorMessage = error instanceof Error ? error.message : '获取收藏夹失败';
			toast.error('获取收藏夹失败', { description: errorMessage });
		} finally {
			loadingFavorites = false;
		}
	}

	// 选择收藏夹
	function selectFavorite(favorite: UserFavoriteFolder) {
		// 检查收藏夹是否已存在
		if (isFavoriteExists(favorite.id)) {
			toast.error('收藏夹已存在', {
				description: `该收藏夹「${favorite.name || favorite.title}」已经添加过了`
			});
			return;
		}

		sourceId = favorite.id.toString();
		name = favorite.name || favorite.title;
		favoriteValidationResult = {
			valid: true,
			fid: favorite.id,
			title: favorite.name || favorite.title,
			message: '收藏夹验证成功'
		};
		toast.success('已选择收藏夹', { description: name });
	}

	// 选择搜索到的收藏夹
	function selectSearchedFavorite(favorite: UserFavoriteFolder) {
		// 检查收藏夹是否已存在
		if (isFavoriteExists(favorite.fid)) {
			toast.error('收藏夹已存在', {
				description: `该收藏夹「${favorite.title}」已经添加过了`
			});
			return;
		}

		sourceId = favorite.fid.toString();
		name = favorite.title;
		favoriteValidationResult = {
			valid: true,
			fid: favorite.fid,
			title: favorite.title,
			message: '收藏夹验证成功'
		};
		toast.success('已选择收藏夹', { description: name });
	}

	// 选择UP主并获取其收藏夹
	async function selectUserAndFetchFavorites(user: SearchResultItem) {
		selectedUserId = user.mid.toString();
		selectedUserName = user.title; // 使用搜索结果中的title

		loadingSearchedUserFavorites = true;
		searchedUserFavorites = [];

		// 关闭搜索结果
		showSearchResults = false;
		searchResults = [];
		searchKeyword = '';
		searchTotalResults = 0;

		try {
			const result = await api.getUserFavoritesByUid(selectedUserId);
			if (result.data && result.data.length > 0) {
				searchedUserFavorites = result.data;
				toast.success('获取收藏夹成功', {
					description: `从 ${selectedUserName} 获取到 ${searchedUserFavorites.length} 个收藏夹`
				});
			} else {
				toast.info('该UP主没有公开收藏夹');
			}
		} catch {
			console.error('获取UP主收藏夹失败');
			toast.error('获取收藏夹失败', {
				description: 'UP主可能没有公开收藏夹或网络错误'
			});
		} finally {
			loadingSearchedUserFavorites = false;
		}
	}

	// 验证收藏夹ID
	async function validateFavoriteId(fid: string) {
		if (!fid.trim()) {
			favoriteValidationResult = null;
			return;
		}

		// 检查是否为纯数字
		if (!/^\d+$/.test(fid.trim())) {
			favoriteValidationResult = {
				valid: false,
				fid: 0,
				title: '',
				message: '收藏夹ID必须为纯数字'
			};
			return;
		}

		validatingFavorite = true;
		favoriteValidationResult = null;

		try {
			const result = await api.validateFavorite(fid.trim());
			favoriteValidationResult = result.data;

			if (result.data.valid && !name) {
				// 如果验证成功且用户还没有填写名称，自动填入收藏夹标题
				name = result.data.title;
			}
		} catch {
			favoriteValidationResult = {
				valid: false,
				fid: parseInt(fid) || 0,
				title: '',
				message: '验证失败：网络错误或收藏夹不存在'
			};
		} finally {
			validatingFavorite = false;
		}
	}

	// 处理收藏夹ID变化
	function handleFavoriteIdChange() {
		clearTimeout(favoriteValidationTimeout);
		if (sourceType === 'favorite' && sourceId.trim()) {
			favoriteValidationTimeout = setTimeout(() => {
				validateFavoriteId(sourceId);
			}, 500);
		} else {
			favoriteValidationResult = null;
		}
	}

	// 处理UP主ID变化
	function handleUpIdChange() {
		clearTimeout(upIdTimeout);
		if (upId.trim()) {
			upIdTimeout = setTimeout(() => {
				fetchUserCollections();
			}, 500);
		} else {
			userCollections = [];
		}
	}

	// 获取UP主合集列表
	async function fetchUserCollections() {
		if (!upId.trim()) return;

		loadingCollections = true;
		try {
			const result = await api.getUserCollections(upId);
			if (result.data && result.data.collections) {
				userCollections = result.data.collections;
				if (userCollections.length === 0) {
					toast.info('该UP主暂无合集');
				} else {
					toast.success('获取合集列表成功', {
						description: `共获取到 ${userCollections.length} 个合集`
					});
				}
			} else {
				toast.error('获取合集列表失败');
				userCollections = [];
			}
		} catch (error: unknown) {
			console.error('获取合集列表失败:', error);

			// 根据错误类型提供更友好的提示
			let errorMessage = '获取合集列表失败';
			let errorDescription = '';

			const errorMsg = error instanceof Error ? error.message : '';

			if (errorMsg === 'Failed to fetch' || errorMsg.includes('ERR_EMPTY_RESPONSE')) {
				errorDescription = '该UP主的合集可能需要登录访问，或暂时无法获取';
			} else if (errorMsg.includes('403') || errorMsg.includes('Forbidden')) {
				errorDescription = '该UP主的合集为私有，无法访问';
			} else if (errorMsg.includes('404') || errorMsg.includes('Not Found')) {
				errorDescription = 'UP主不存在或合集已被删除';
			} else {
				errorDescription = '网络错误或服务暂时不可用，请稍后重试';
			}

			toast.error(errorMessage, { description: errorDescription });
			userCollections = [];
		} finally {
			loadingCollections = false;
		}
	}

	// 选择合集
	function selectCollection(collection: UserCollectionItem) {
		// 检查合集是否已存在
		if (isCollectionExists(collection.sid, collection.mid.toString())) {
			toast.error('合集已存在', {
				description: `该合集「${collection.name}」已经添加过了`
			});
			return;
		}

		sourceId = collection.sid;
		name = collection.name;
		cover = collection.cover || '';
		collectionType = collection.collection_type;
		isManualInput = false; // 从列表选择，不是手动输入
		toast.success('已选择合集', {
			description: `${collection.collection_type === 'season' ? '合集' : '系列'}：${collection.name}`
		});
	}


	// 加载已有视频源（用于过滤）
	async function loadExistingVideoSources() {
		loadingExistingSources = true;
		try {
			const result = await api.getVideoSources();
			if (result.data) {
				existingVideoSources = result.data;

				// 处理合集：存储 s_id_m_id 的组合
				existingCollectionIds.clear();
				result.data.collection?.forEach((c) => {
					if (c.s_id && c.m_id) {
						const key = `${c.s_id}_${c.m_id}`;
						existingCollectionIds.add(key);
					}
				});

				// 处理收藏夹
				existingFavoriteIds.clear();
				result.data.favorite?.forEach((f) => {
					if (f.f_id) {
						existingFavoriteIds.add(f.f_id);
					}
				});

				// 处理UP主投稿
				existingSubmissionIds.clear();
				result.data.submission?.forEach((s) => {
					if (s.upper_id) {
						existingSubmissionIds.add(s.upper_id);
					}
				});

			}
		} catch (error) {
			console.error('加载已有视频源失败:', error);
		} finally {
			loadingExistingSources = false;
		}
	}

	// 检查合集是否已存在
	function isCollectionExists(sId: string, mId: string): boolean {
		const key = `${sId}_${mId}`;
		return existingCollectionIds.has(key);
	}

	// 检查UP主投稿是否已存在
	function isSubmissionExists(upperId: number): boolean {
		return existingSubmissionIds.has(upperId);
	}

	// 检查收藏夹是否已存在
	function isFavoriteExists(fId: number): boolean {
		return existingFavoriteIds.has(fId);
	}


	// 过滤后的收藏夹列表（未添加的排在前面，已添加的排在后面）
	$: filteredUserFavorites = (() => {
		const notAdded = userFavorites.filter((fav) => !existingFavoriteIds.has(Number(fav.id)));
		const added = userFavorites.filter((fav) => existingFavoriteIds.has(Number(fav.id)));
		return [...notAdded, ...added];
	})();

	$: filteredSearchedUserFavorites = (() => {
		const notAdded = searchedUserFavorites.filter((fav) => !existingFavoriteIds.has(Number(fav.id)));
		const added = searchedUserFavorites.filter((fav) => existingFavoriteIds.has(Number(fav.id)));
		return [...notAdded, ...added];
	})();

	// 过滤后的合集列表（未添加的排在前面，已添加的排在后面）
	$: filteredUserCollections = (() => {
		const notAdded = userCollections.filter((col) =>
			!isCollectionExists(col.sid, col.mid.toString())
		);
		const added = userCollections.filter((col) =>
			isCollectionExists(col.sid, col.mid.toString())
		);
		return [...notAdded, ...added];
	})();

	// 过滤后的关注UP主列表（未添加的排在前面，已添加的排在后面）
	$: filteredUserFollowings = (() => {
		const notAdded = userFollowings.filter((user) =>
			user.mid && !existingSubmissionIds.has(Number(user.mid))
		);
		const added = userFollowings.filter((user) =>
			user.mid && existingSubmissionIds.has(Number(user.mid))
		);
		return [...notAdded, ...added];
	})();

	// 过滤后的搜索结果（根据类型过滤已存在的源）
	$: filteredSearchResults = (() => {
		// 对于UP主投稿类型，将未添加的UP主排在前面，已添加的排在后面
		if (sourceType === 'submission') {
			const notAdded = searchResults.filter((result) =>
				result.mid && !existingSubmissionIds.has(Number(result.mid))
			);
			const added = searchResults.filter((result) =>
				result.mid && existingSubmissionIds.has(Number(result.mid))
			);
			return [...notAdded, ...added];
		}
		// 对于番剧和合集搜索，不完全过滤，显示所有结果但标记已存在状态
		return searchResults;
	})();


	// 监听sourceType变化，重置手动输入标志和清空所有缓存
	$: if (sourceType) {
		isManualInput = false;
		// 清空搜索相关状态
		searchResults = [];
		searchKeyword = '';
		searchTotalResults = 0;
		showSearchResults = false;
		hoveredItem = null;
		// 清空各类型的缓存数据
		userFollowings = [];
		userCollections = [];
		userFavorites = [];
		subscribedCollections = [];
		// 清空UP主收藏夹搜索状态
		searchedUserFavorites = [];
		selectedUserId = '';
		selectedUserName = '';
		loadingSearchedUserFavorites = false;
	}

	// 切换源类型时，如处于批量模式且已有选择，则清空选择防止跨源类型
	$: if (sourceType !== lastSourceType) {
		if (batchMode && batchSelectedItems.size > 0) {
			clearBatchSelection();
			toast('已切换源类型，已清空批量选择，请重新选择');
		}
		lastSourceType = sourceType;
	}

	// 统一的悬浮处理函数
	function handleItemMouseEnter(
		type: 'search',
		data: SearchResultItem,
		event: MouseEvent
	) {
		hoveredItem = { type, data };
		updateTooltipPosition(event);
	}

	function handleItemMouseMove(event: MouseEvent) {
		if (hoveredItem) {
			updateTooltipPosition(event);
		}
	}

	function updateTooltipPosition(event: MouseEvent) {
		// 获取视窗尺寸
		const viewportWidth = window.innerWidth;
		const viewportHeight = window.innerHeight;
		const tooltipWidth = 400; // 预估悬浮窗宽度
		const tooltipHeight = 300; // 预估悬浮窗高度

		let x = event.pageX + 20;
		let y = event.pageY - 100;

		// 防止悬浮窗超出右边界
		if (x + tooltipWidth > viewportWidth) {
			x = event.pageX - tooltipWidth - 20;
		}

		// 防止悬浮窗超出下边界
		if (y + tooltipHeight > viewportHeight) {
			y = event.pageY - tooltipHeight - 20;
		}

		// 防止悬浮窗超出上边界和左边界
		mousePosition = {
			x: Math.max(10, x),
			y: Math.max(10, y)
		};
	}

	function handleItemMouseLeave() {
		hoveredItem = null;
	}

	// 为了向后兼容，保留旧的函数名但重定向到新的统一函数
	function handleMouseEnter(result: SearchResultItem, event: MouseEvent) {
		handleItemMouseEnter('search', result, event);
	}

	function handleMouseMove(event: MouseEvent) {
		handleItemMouseMove(event);
	}

	function handleMouseLeave() {
		handleItemMouseLeave();
	}

	// 获取关注的UP主列表
	async function fetchUserFollowings() {
		loadingFollowings = true;
		try {
			const result = await api.getUserFollowings();
			if (result.data) {
				userFollowings = result.data;
				toast.success('获取关注UP主成功', {
					description: `共获取到 ${userFollowings.length} 个UP主`
				});
			} else {
				toast.error('获取关注UP主失败');
			}
		} catch (error: unknown) {
			console.error('获取关注UP主失败:', error);
			const errorMessage = error instanceof Error ? error.message : '获取关注UP主失败';
			toast.error('获取关注UP主失败', { description: errorMessage });
		} finally {
			loadingFollowings = false;
		}
	}

	// 选择关注的UP主
	function selectFollowing(following: UserFollowing) {
		switch (sourceType) {
			case 'collection':
				upId = following.mid.toString();
				// 触发获取UP主合集列表
				handleUpIdChange();
				toast.success('已填充UP主信息', { description: '正在获取合集列表...' });
				break;
			case 'submission':
				sourceId = following.mid.toString();
				name = following.name;
				selectedUpName = following.name;
				// 打开投稿选择对话框
				showSubmissionSelection = true;
				toast.success('已填充UP主信息');
				break;
		}

		// 清空关注UP主列表状态，关闭面板
		userFollowings = [];
	}

	// 获取关注的收藏夹列表
	async function fetchSubscribedCollections() {
		loadingSubscribedCollections = true;
		try {
			const result = await api.getSubscribedCollections();
			if (result.data) {
				subscribedCollections = result.data;
				if (subscribedCollections.length === 0) {
					toast.info('暂无关注的合集', {
						description: '您还没有关注任何合集。关注合集后可以在这里快速选择添加。',
						duration: 5000
					});
				} else {
					toast.success('获取关注的合集成功', {
						description: `共获取到 ${subscribedCollections.length} 个您关注的合集`
					});
				}
			} else {
				toast.error('获取合集失败');
			}
		} catch (error: unknown) {
			console.error('获取合集失败:', error);
			const errorMessage = error instanceof Error ? error.message : '获取合集失败';
			toast.error('获取合集失败', { description: errorMessage });
		} finally {
			loadingSubscribedCollections = false;
		}
	}

	// 选择订阅的合集
	function selectSubscribedCollection(collection: UserCollectionInfo) {
		sourceId = collection.sid;
		name = collection.name;
		cover = collection.cover || '';
		upId = collection.up_mid.toString();
		collectionType = collection.collection_type;
		toast.success('已选择订阅合集', { description: collection.name });
	}

	// 处理投稿选择确认
	// eslint-disable-next-line @typescript-eslint/no-unused-vars
	function handleSubmissionSelectionConfirm(selectedBvids: string[]) {
		selectedVideos = selectedBvids;
		showSubmissionSelection = false;
		if (selectedBvids.length > 0) {
			toast.success('已选择投稿', {
				description: `选择了 ${selectedBvids.length} 个历史投稿，新投稿将自动下载`
			});
		} else {
			toast.info('未选择投稿', {
				description: '将下载所有历史投稿和新投稿'
			});
		}
	}

	// 处理投稿选择取消
	// eslint-disable-next-line @typescript-eslint/no-unused-vars
	function handleSubmissionSelectionCancel() {
		showSubmissionSelection = false;
		// 保留已有的选择，不做清空
	}

	// 投稿选择相关函数

	// 重置投稿选择状态
	function resetSubmissionState() {
		submissionVideos = [];
		selectedSubmissionVideos = new Set();
		submissionLoading = false;
		submissionError = null;
		submissionTotalCount = 0;
		submissionSearchQuery = '';
		filteredSubmissionVideos = [];
	}

	// 搜索相关状态
	let searchTimeout: NodeJS.Timeout;
	let isSearching = false;

	// 搜索过滤投稿 - 使用后端API搜索
	// eslint-disable-next-line svelte/infinite-reactive-loop
	$: {
		if (submissionSearchQuery.trim()) {
			// 清除之前的搜索定时器
			if (searchTimeout) {
				clearTimeout(searchTimeout);
			}

			// 设置新的搜索定时器（防抖）
			searchTimeout = setTimeout(() => {
				// eslint-disable-next-line svelte/infinite-reactive-loop
				performSearch();
			}, 500); // 500ms防抖
		} else {
			filteredSubmissionVideos = submissionVideos;
		}
	}

	// 执行搜索
	/* eslint-disable svelte/infinite-reactive-loop */
	async function performSearch() {
		if (!sourceId || !submissionSearchQuery.trim()) {
			filteredSubmissionVideos = submissionVideos;
			return;
		}

		isSearching = true;

		try {
			const response = await api.getSubmissionVideos({
				up_id: sourceId,
				page: 1,
				page_size: 30, // 获取更多结果
				keyword: submissionSearchQuery.trim()
			});

			if (response.data && response.data.videos) {
				filteredSubmissionVideos = response.data.videos;
			} else {
				filteredSubmissionVideos = [];
			}
		} catch (error) {
			console.error('搜索视频失败:', error);
			toast.error('搜索失败', {
				description: '请稍后重试'
			});
			// 搜索失败时回退到本地过滤
			filteredSubmissionVideos = submissionVideos.filter((video) =>
				video.title.toLowerCase().includes(submissionSearchQuery.toLowerCase().trim())
			);
		} finally {
			isSearching = false;
		}
	}
	/* eslint-enable svelte/infinite-reactive-loop */

	// 加载UP主投稿列表（分页加载，初始100个）
	async function loadSubmissionVideos() {
		if (!sourceId) return;

		submissionLoading = true;
		submissionError = null;
		submissionVideos = [];
		currentLoadedPage = 0;
		hasMoreVideos = true;
		showLoadMoreButton = false;

		try {
			await loadVideosInBatch(INITIAL_LOAD_SIZE);
		} catch (err) {
			submissionError = err instanceof Error ? err.message : '网络请求失败';
		} finally {
			submissionLoading = false;
		}
	}

	// 批量加载视频（串行请求，带延迟）
	async function loadVideosInBatch(loadCount: number) {
		const startPage = currentLoadedPage + 1;
		const targetVideos = Math.min(
			submissionVideos.length + loadCount,
			submissionTotalCount || Infinity
		);
		const neededPages = Math.ceil(targetVideos / SUBMISSION_PAGE_SIZE);

		for (let page = startPage; page <= neededPages; page++) {
			// 更新进度
			loadingProgress = `正在加载第 ${page} 页...`;

			// 延迟（除了第一页）
			if (page > startPage) {
				await new Promise((resolve) => setTimeout(resolve, PAGE_DELAY));
			}

			const response = await api.getSubmissionVideos({
				up_id: sourceId,
				page: page,
				page_size: SUBMISSION_PAGE_SIZE
			});

			if (!response.data) {
				throw new Error('获取投稿列表失败');
			}

			// 第一次请求时获取总数
			if (page === 1 && submissionTotalCount === 0) {
				submissionTotalCount = response.data.total;
			}

			// 添加新视频（去重）
			const newVideos = response.data.videos || [];
			const existingBvids = new Set(submissionVideos.map((v) => v.bvid));
			const uniqueNewVideos = newVideos.filter((video) => !existingBvids.has(video.bvid));

			submissionVideos = [...submissionVideos, ...uniqueNewVideos];
			currentLoadedPage = page;

			// 检查是否达到目标数量或已加载全部
			if (
				submissionVideos.length >= targetVideos ||
				submissionVideos.length >= submissionTotalCount
			) {
				break;
			}
		}

		// 更新状态
		hasMoreVideos = submissionVideos.length < submissionTotalCount;
		// 不自动显示按钮，等待用户滚动到底部时才显示
		loadingProgress = '';
	}

	// 加载更多投稿视频
	async function loadMoreSubmissionVideos() {
		if (!hasMoreVideos || isLoadingMore) return;

		isLoadingMore = true;
		showLoadMoreButton = false; // 隐藏按钮
		try {
			await loadVideosInBatch(LOAD_MORE_SIZE);
		} catch (err) {
			console.error('加载更多视频失败:', err);
			toast.error('加载更多视频失败', {
				description: err instanceof Error ? err.message : '网络请求失败'
			});
		} finally {
			isLoadingMore = false;
		}
	}

	// 加载所有剩余视频
	async function loadAllRemainingVideos() {
		if (!hasMoreVideos || isLoadingMore) return;

		isLoadingMore = true;
		showLoadMoreButton = false;
		try {
			// 计算剩余需要加载的视频数量
			const remaining = submissionTotalCount - submissionVideos.length;
			if (remaining > 0) {
				toast.info(`开始加载剩余 ${remaining} 个视频...`);
				await loadVideosInBatch(remaining);
				toast.success('所有视频加载完成');
			}
		} catch (err) {
			console.error('加载所有视频失败:', err);
			toast.error('加载所有视频失败', {
				description: err instanceof Error ? err.message : '网络请求失败'
			});
		} finally {
			isLoadingMore = false;
		}
	}

	// 处理滚动事件，检测是否需要显示加载更多按钮
	function handleSubmissionScroll(event: Event) {
		const container = event.target as HTMLElement;
		if (!container || !hasMoreVideos) return;

		const { scrollTop, scrollHeight, clientHeight } = container;
		const threshold = 100; // 距离底部100px时显示按钮

		// 当滚动接近底部时显示加载更多按钮
		if (scrollHeight - scrollTop - clientHeight < threshold) {
			showLoadMoreButton = true;
		}
	}

	// 处理视频选择
	function toggleSubmissionVideo(bvid: string) {
		if (selectedSubmissionVideos.has(bvid)) {
			selectedSubmissionVideos.delete(bvid);
		} else {
			selectedSubmissionVideos.add(bvid);
		}
		selectedSubmissionVideos = selectedSubmissionVideos; // 触发响应式更新
	}

	// 全选投稿
	function selectAllSubmissions() {
		filteredSubmissionVideos.forEach((video) => selectedSubmissionVideos.add(video.bvid));
		selectedSubmissionVideos = selectedSubmissionVideos;
	}

	// 全不选投稿
	function selectNoneSubmissions() {
		filteredSubmissionVideos.forEach((video) => selectedSubmissionVideos.delete(video.bvid));
		selectedSubmissionVideos = selectedSubmissionVideos;
	}

	// 反选投稿
	function invertSubmissionSelection() {
		filteredSubmissionVideos.forEach((video) => {
			if (selectedSubmissionVideos.has(video.bvid)) {
				selectedSubmissionVideos.delete(video.bvid);
			} else {
				selectedSubmissionVideos.add(video.bvid);
			}
		});
		selectedSubmissionVideos = selectedSubmissionVideos;
	}

	// 确认投稿选择
	function confirmSubmissionSelection() {
		selectedVideos = Array.from(selectedSubmissionVideos);
		showSubmissionSelection = false;
		if (selectedVideos.length > 0) {
			toast.success('已选择投稿', {
				description: `选择了 ${selectedVideos.length} 个历史投稿，新投稿将自动下载`
			});
		} else {
			toast.info('未选择投稿', {
				description: '将下载所有历史投稿和新投稿'
			});
		}
	}

	// 取消投稿选择
	function cancelSubmissionSelection() {
		showSubmissionSelection = false;
		// 保留已有的选择，不做清空
	}

	// 格式化时间
	function formatSubmissionDate(pubtime: string): string {
		try {
			return new Date(pubtime).toLocaleDateString('zh-CN');
		} catch {
			return pubtime;
		}
	}

	// 格式化播放量
	function formatSubmissionPlayCount(count: number): string {
		if (count >= 10000) {
			return (count / 10000).toFixed(1) + '万';
		}
		return count.toString();
	}

	// 当显示投稿选择且有sourceId时加载数据
	$: if (showSubmissionSelection && sourceId && sourceType === 'submission') {
		resetSubmissionState();
		loadSubmissionVideos();
	}

	// 计算已选择的投稿数量
	$: selectedSubmissionCount = Array.from(selectedSubmissionVideos).filter((bvid) =>
		filteredSubmissionVideos.some((video) => video.bvid === bvid)
	).length;

	// 批量选择相关函数
	function resolveBatchItemSourceTypeByRawType(itemType: string): string {
		// 将原始的批量项类别映射为其对应的视频源类型
		switch (itemType) {
			case 'search':
			case 'following':
				return sourceType; // 跟随当前选择的源类型
			case 'favorite':
				return 'favorite';
			case 'collection':
				return 'collection';
			default:
				return sourceType;
		}
	}
	function toggleBatchSelection(itemKey: string, item: any, itemType: string) {
		console.log('🔵 toggleBatchSelection called with:', {
			itemKey,
			itemType,
			batchMode,
			currentMapSize: batchSelectedItems.size
		});

		if (batchSelectedItems.has(itemKey)) {
			console.log('🔴 Removing item:', itemKey);
			batchSelectedItems.delete(itemKey);
		} else {
			console.log('🟢 Adding item:', itemKey);

			// 先做跨源类型限制：不允许在一次批量中混合不同视频源类型
			const newItemSourceType = resolveBatchItemSourceTypeByRawType(itemType);
			if (batchSelectedItems.size > 0) {
				const first = batchSelectedItems.values().next().value;
				const currentBatchSourceType = getSourceTypeFromBatchItem(first);
				if (newItemSourceType !== currentBatchSourceType) {
					console.log('❌ Cross-type selection rejected');
					toast.error('批量模式不支持跨源类型选择', {
						description: '请先清空已选项，再选择其他源类型的内容'
					});
					return;
				}
			}

			// 生成默认名称
			let itemName = '';

			switch (itemType) {
				case 'search':
					itemName = cleanTitle(item.title);
					break;
				case 'favorite':
					itemName = item.name || item.title;
					break;
				case 'collection':
					itemName = item.name || item.title;
					break;
				case 'following':
					itemName = item.name;
					break;
			}

			batchSelectedItems.set(itemKey, {
				type: itemType,
				data: item,
				name: itemName
			});
		}

		console.log('📊 After operation:', {
			newMapSize: batchSelectedItems.size,
			hasItem: batchSelectedItems.has(itemKey),
			allKeys: Array.from(batchSelectedItems.keys())
		});

		batchSelectedItems = new Map(batchSelectedItems);
		console.log('✅ New Map created for reactivity');
	}

	function isBatchSelected(itemKey: string): boolean {
		return !!batchCheckboxStates[itemKey];
	}

	function clearBatchSelection() {
		console.log('🧹 Clearing batch selection');
		batchSelectedItems = new Map();
		console.log('✅ New empty Map created');
	}

	function selectAllVisible(itemType: string) {
		switch (itemType) {
			case 'search':
				filteredSearchResults.forEach((result, index) => {
					const key = `search_${result.bvid || result.season_id || result.mid || index}`;
					if (!batchSelectedItems.has(key)) {
						toggleBatchSelection(key, result, 'search');
					}
				});
				break;
			case 'following':
				filteredUserFollowings.forEach((following) => {
					const key = `following_${following.mid}`;
					// 跳过已添加的UP主
					const isDisabled =
						sourceType === 'submission' && existingSubmissionIds.has(following.mid);
					if (!batchSelectedItems.has(key) && !isDisabled) {
						toggleBatchSelection(key, following, 'following');
					}
				});
				break;
			case 'favorite':
				userFavorites.forEach((favorite) => {
					const key = `favorite_${favorite.id}`;
					// 跳过已添加的收藏夹
					const isDisabled = existingFavoriteIds.has(favorite.id);
					if (!batchSelectedItems.has(key) && !isDisabled) {
						toggleBatchSelection(key, favorite, 'favorite');
					}
				});
				break;
			case 'searched-favorite':
				searchedUserFavorites.forEach((favorite) => {
					const key = `searched-favorite_${favorite.fid}`;
					// 跳过已添加的收藏夹
					const isDisabled = existingFavoriteIds.has(Number(favorite.fid));
					if (!batchSelectedItems.has(key) && !isDisabled) {
						toggleBatchSelection(key, favorite, 'favorite');
					}
				});
				break;
			case 'collection':
				userCollections.forEach((collection) => {
					const key = `collection_${collection.sid}`;
					// 跳过已添加的合集
					const isDisabled = isCollectionExists(collection.sid, collection.mid.toString());
					if (!batchSelectedItems.has(key) && !isDisabled) {
						toggleBatchSelection(key, collection, 'collection');
					}
				});
				break;
			case 'subscribed-collection':
				subscribedCollections.forEach((collection) => {
					const key = `subscribed-collection_${collection.sid}`;
					// 跳过已添加的合集
					const isDisabled = isCollectionExists(collection.sid, collection.up_mid.toString());
					if (!batchSelectedItems.has(key) && !isDisabled) {
						toggleBatchSelection(key, collection, 'collection');
					}
				});
				break;
		}
	}

	// 批量添加函数
	async function handleBatchAdd() {
		if (batchSelectedItems.size === 0) {
			toast.error('未选择任何视频源');
			return;
		}

		// 校验所有被选项是否属于同一视频源类型，防止跨源类型批量添加
		const resolvedTypes = new Set(
			Array.from(batchSelectedItems.values()).map((it: any) => getSourceTypeFromBatchItem(it))
		);
		if (resolvedTypes.size > 1) {
			toast.error('不能跨源类型批量添加', {
				description: '已检测到多种源类型，请清空后按类型分别添加'
			});
			return;
		}
		batchAdding = true;
		batchProgress = { current: 0, total: batchSelectedItems.size };

		const selectedItems = Array.from(batchSelectedItems.entries());
		let successCount = 0;
		let failedCount = 0;
		const failedItems: string[] = [];

		try {
			for (let i = 0; i < selectedItems.length; i++) {
				const [itemKey, item] = selectedItems[i];
				batchProgress.current = i + 1;

				try {
					// 构建添加视频源的参数
					const params: AddVideoSourceRequest = {
						source_type: getSourceTypeFromBatchItem(item),
						source_id: getSourceIdFromBatchItem(item),
						name: item.name,
						path: batchBasePath
					};

					// 添加特定类型的额外参数
					if (item.type === 'following') {
						if (sourceType === 'collection') {
							params.up_id = item.data.mid.toString();
							params.collection_type = 'season';
						} else if (sourceType === 'submission') {
							// 批量添加UP主投稿时总是使用全部投稿模式
						}
					} else if (item.type === 'collection') {
						// 区分普通合集和关注的合集
						if (itemKey.startsWith('subscribed-collection_')) {
							// 关注的合集使用 up_mid
							params.up_id = item.data.up_mid.toString();
							params.collection_type = item.data.collection_type || 'season';
						} else {
							// 普通合集使用 mid
							params.up_id = item.data.mid.toString();
							params.collection_type = item.data.type || 'season';
						}
					}

					if (item.data.cover) {
						params.cover = item.data.cover;
					}

					const result = await api.addVideoSource(params);

					if (result.data.success) {
						successCount++;
					} else {
						failedCount++;
						failedItems.push(item.name);
					}
				} catch (error) {
					failedCount++;
					failedItems.push(item.name);
					console.error(`添加视频源 ${item.name} 失败:`, error);
				}

				// 添加小延迟避免请求过于频繁
				await new Promise((resolve) => setTimeout(resolve, 200));
			}

			// 显示结果
			if (successCount > 0 && failedCount === 0) {
				toast.success('批量添加完成', {
					description: `成功添加 ${successCount} 个视频源`
				});
			} else if (successCount > 0) {
				toast.success('批量添加部分成功', {
					description: `成功添加 ${successCount} 个，失败 ${failedCount} 个`
				});
			} else {
				toast.error('批量添加失败', {
					description: '所有视频源都添加失败'
				});
			}

			// 清空选择并关闭批量模式
			clearBatchSelection();
			batchMode = false;
			batchDialogOpen = false;

			// 如果有成功添加的，跳转到视频源管理页面
			if (successCount > 0) {
				setTimeout(() => {
					goto('/video-sources');
				}, 1000);
			}
		} catch (error) {
			console.error('批量添加过程中发生错误:', error);
			toast.error('批量添加失败', {
				description: '添加过程中发生错误'
			});
		} finally {
			batchAdding = false;
			batchProgress = { current: 0, total: 0 };
		}
	}

	// 根据批量选择项获取视频源类型
	function getSourceTypeFromBatchItem(item: any): string {
		switch (item.type) {
			case 'search':
				return sourceType; // 使用当前选择的源类型
			case 'following':
				return sourceType; // 使用当前选择的源类型（submission 或 collection）
			case 'favorite':
				return 'favorite';
			case 'collection':
				return 'collection';
			default:
				return sourceType;
		}
	}

	// 根据批量选择项获取视频源ID
	function getSourceIdFromBatchItem(item: any): string {
		switch (item.type) {
			case 'search':
				if (sourceType === 'submission') {
					return item.data.mid?.toString() || '';
				}
				return item.data.bvid || item.data.season_id || item.data.mid?.toString() || '';
			case 'following':
				return item.data.mid.toString();
			case 'favorite':
				// 处理两种收藏夹数据结构：用户自己的收藏夹使用id，搜索到的收藏夹使用fid
				return (item.data.fid || item.data.id).toString();
			case 'collection':
				return item.data.sid.toString();
			default:
				return '';
		}
	}
</script>

<svelte:head>
	<title>添加视频源 - Bili Sync</title>
</svelte:head>

<svelte:window bind:innerWidth />

<div class="py-2">
	<div class="mx-auto px-4">
		<div class="bg-card rounded-lg border p-6 shadow-sm">
			<div class="mb-6 flex items-center justify-between">
				<h1 class="text-2xl font-bold">添加新视频源</h1>
				{#if sourceType !== 'watch_later'}
					<Button
						variant={batchMode ? 'default' : 'outline'}
						size="sm"
						onclick={() => {
							batchMode = !batchMode;
							if (!batchMode) {
								batchSelectedItems.clear();
								batchSelectedItems = batchSelectedItems;
							}
						}}
						class="flex items-center gap-2"
					>
						{#if batchMode}
							<X class="h-4 w-4" />
							退出批量模式
						{:else}
							<PlusIcon class="h-4 w-4" />
							批量添加
						{/if}
					</Button>
				{/if}
			</div>

			<div class="flex {isMobile ? 'flex-col' : 'gap-8'}">
				<!-- 左侧：表单区域 -->
				<div class={isMobile ? 'w-full' : 'max-w-[500px] min-w-[350px] flex-1'}>
					<form
						onsubmit={(e) => {
							e.preventDefault();
							handleSubmit();
						}}
						class="space-y-6"
					>
						<!-- 视频源类型 -->
						<div class="space-y-2">
							<Label for="source-type">视频源类型</Label>
							<select
								id="source-type"
								bind:value={sourceType}
								class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
							>
								{#each sourceTypeOptions as option (option.value)}
									<option value={option.value}>{option.label}</option>
								{/each}
							</select>
							<p class="text-muted-foreground text-sm">{currentTypeDescription}</p>
						</div>

						<!-- 搜索功能 -->
						{#if sourceType !== 'favorite' && sourceType !== 'watch_later'}
							<div
								class="rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-950"
							>
								<div class="space-y-2">
									<div>
										<Label for="search">
											{#if sourceType === 'collection'}
												搜索UP主
											{:else if sourceType === 'submission'}
												搜索UP主
											{:else}
												搜索B站内容
											{/if}
										</Label>
										<div class="flex {isMobile ? 'flex-col gap-2' : 'gap-2'} mt-2">
											<Input
												id="search"
												bind:value={searchKeyword}
												placeholder={sourceType === 'submission' || sourceType === 'collection'
													? '搜索UP主...'
													: '搜索视频...'}
												onkeydown={(e) => e.key === 'Enter' && handleSearch()}
											/>
											<div class="flex gap-2">
												<Button
													onclick={() => handleSearch()}
													disabled={searchLoading || !searchKeyword.trim()}
													size="sm"
													class={isMobile ? 'flex-1' : ''}
												>
													{#if searchLoading}
														搜索中...
													{:else}
														<Search class="h-4 w-4" />
													{/if}
												</Button>
												{#if sourceType === 'collection' || sourceType === 'submission'}
													<Button
														onclick={sourceType === 'collection'
															? fetchSubscribedCollections
															: fetchUserFollowings}
														disabled={sourceType === 'collection'
															? loadingSubscribedCollections
															: loadingFollowings}
														size="sm"
														variant="outline"
														class={isMobile ? 'flex-1' : ''}
													>
														{sourceType === 'collection'
															? loadingSubscribedCollections
																? '获取中...'
																: '获取关注的合集'
															: loadingFollowings
																? '获取中...'
																: '获取关注'}
													</Button>
												{/if}
											</div>
										</div>
										<p class="text-muted-foreground mt-1 text-xs">
											{#if sourceType === 'collection'}
												搜索UP主后会自动填充UP主ID，并显示该UP主的所有合集供选择
											{:else if sourceType === 'submission'}
												搜索并选择UP主，将自动填充UP主ID
											{:else}
												根据当前选择的视频源类型搜索对应内容
											{/if}
										</p>
									</div>
								</div>
							</div>
						{/if}

						<!-- 收藏夹列表（仅收藏夹类型时显示） -->
						{#if sourceType === 'favorite'}
							<div class="space-y-4">
								<!-- 我的收藏夹 -->
								<div
									class="rounded-lg border border-yellow-200 bg-yellow-50 p-4 dark:border-yellow-800 dark:bg-yellow-950"
								>
									<div
										class="flex {isMobile ? 'flex-col gap-2' : 'items-center justify-between'} mb-2"
									>
										<span class="text-sm font-medium text-yellow-800 dark:text-yellow-200"
											>我的收藏夹</span
										>
										<Button
											size="sm"
											variant="outline"
											onclick={fetchUserFavorites}
											disabled={loadingFavorites}
											class={isMobile ? 'w-full' : ''}
										>
											{loadingFavorites ? '加载中...' : '获取收藏夹'}
										</Button>
									</div>

									{#if userFavorites.length > 0}
										<p class="text-xs text-yellow-600 dark:text-yellow-400">
											已获取 {userFavorites.length} 个收藏夹，请在{isMobile ? '下方' : '右侧'}选择
										</p>
									{:else}
										<p class="text-xs text-yellow-600 dark:text-yellow-400">
											点击右侧按钮获取您的收藏夹列表
										</p>
									{/if}
								</div>

								<!-- 他人的公开收藏夹 -->
								<div
									class="rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-950"
								>
									<div class="mb-3">
										<span class="text-sm font-medium text-blue-800 dark:text-blue-200"
											>他人的公开收藏夹</span
										>
									</div>

									<!-- 搜索UP主的收藏夹 -->
									<div class="bg-card mb-4 rounded border border-gray-200 p-3">
										<div class="mb-2">
											<Label class="text-foreground text-sm font-medium">搜索UP主的收藏夹</Label>
										</div>
										<div class="flex {isMobile ? 'flex-col gap-2' : 'gap-2'}">
											<Input
												placeholder="搜索UP主名称..."
												bind:value={searchKeyword}
												onkeydown={(e) => e.key === 'Enter' && handleSearch()}
											/>
											<Button
												onclick={() => handleSearch()}
												disabled={searchLoading || !searchKeyword.trim()}
												size="sm"
												class={isMobile ? 'w-full' : ''}
											>
												{#if searchLoading}搜索中...{:else}搜索{/if}
											</Button>
										</div>

										<p class="text-muted-foreground mt-2 text-xs">
											{#if showSearchResults && searchResults.length > 0}
												找到 {searchResults.length} 个UP主，请在{isMobile
													? '下方'
													: '右侧'}列表中选择
											{:else}
												输入UP主名称后点击搜索，结果将在{isMobile ? '下方' : '右侧'}显示
											{/if}
										</p>
									</div>

									<!-- 手动输入收藏夹ID -->
									<div class="text-xs text-blue-600 dark:text-blue-400">
										<strong>或者手动输入收藏夹ID：</strong><br />
										1. 打开想要添加的收藏夹页面<br />
										2. 复制URL中 "fid=" 后面的数字<br />
										3. 在下方输入框中填写该数字
									</div>
								</div>
							</div>
						{/if}

						<!-- 合集类型（仅合集时显示，且手动输入） -->
						{#if sourceType === 'collection' && isManualInput}
							<div class="space-y-2">
								<Label for="collection-type">合集类型</Label>
								<select
									id="collection-type"
									bind:value={collectionType}
									class="border-input bg-background ring-offset-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none"
								>
									{#each collectionTypeOptions as option (option.value)}
										<option value={option.value}>{option.label}</option>
									{/each}
								</select>
								<p class="text-sm text-orange-600">
									⚠️ 手动输入合集ID时需要指定类型，建议从{isMobile ? '下方' : '右侧'}合集列表中选择
								</p>
							</div>
						{/if}

						<!-- UP主ID（仅合集时显示） -->
						{#if sourceType === 'collection'}
							<div class="space-y-2">
								<Label for="up-id">UP主ID</Label>
								<Input
									id="up-id"
									bind:value={upId}
									placeholder="请输入UP主ID"
									onblur={handleUpIdChange}
									required
								/>
								{#if userCollections.length > 0}
									<p class="mt-1 text-xs text-green-600">
										✓ 已获取合集列表，请在{isMobile ? '下方' : '右侧'}选择
									</p>
								{/if}
							</div>
						{/if}

						<!-- 视频源ID（稍后观看除外） -->
						{#if sourceType !== 'watch_later'}
							<div class="space-y-2">
								<Label for="source-id">
									{#if sourceType === 'collection'}合集ID
									{:else if sourceType === 'favorite'}收藏夹ID
									{:else if sourceType === 'submission'}UP主ID
									{:else}ID{/if}
								</Label>
								<Input
									id="source-id"
									bind:value={sourceId}
									placeholder={`请输入${sourceType === 'collection' ? '合集' : sourceType === 'favorite' ? '任意公开收藏夹' : sourceType === 'submission' ? 'UP主' : ''}ID`}
									oninput={() => {
										if (sourceType === 'collection') {
											isManualInput = true;
										} else if (sourceType === 'favorite') {
											handleFavoriteIdChange();
										}
									}}
									required
								/>
								{#if sourceType === 'collection' && !isManualInput && sourceId}
									<p class="mt-1 text-xs text-green-600">✓ 已从列表中选择合集，类型已自动识别</p>
								{/if}
								{#if sourceType === 'favorite' && sourceId}
									{#if validatingFavorite}
										<p class="mt-1 text-xs text-blue-600 dark:text-blue-400">🔍 验证收藏夹中...</p>
									{:else if favoriteValidationResult}
										{#if favoriteValidationResult.valid}
											<p class="mt-1 text-xs text-green-600">
												✓ 收藏夹验证成功：{favoriteValidationResult.title}
											</p>
										{:else}
											<p class="mt-1 text-xs text-red-600">✗ {favoriteValidationResult.message}</p>
										{/if}
									{/if}
								{/if}


								<!-- UP主投稿选择状态显示和控制（仅投稿类型时显示） -->
								{#if sourceType === 'submission' && sourceId}
									<div
										class="mt-3 rounded-lg border border-blue-200 bg-blue-50 p-3 dark:border-blue-800 dark:bg-blue-950"
									>
										<div class="flex items-center justify-between">
											<div>
												<span class="text-sm font-medium text-blue-800 dark:text-blue-200"
													>历史投稿选择</span
												>
												<span class="ml-2 text-xs text-blue-600 dark:text-blue-400">
													{#if selectedVideos.length > 0}
														已选择 {selectedVideos.length} 个历史投稿
													{:else}
														未选择特定投稿（将下载全部）
													{/if}
												</span>
											</div>
											<Button
												size="sm"
												variant="outline"
												onclick={() => {
													showSubmissionSelection = true;
												}}
												class="border-blue-300 text-blue-700 hover:bg-blue-100"
											>
												{selectedVideos.length > 0 ? '重新选择' : '选择投稿'}
											</Button>
										</div>
										<p class="mt-2 text-xs text-blue-600 dark:text-blue-400">
											💡
											您可以选择特定的历史投稿进行下载，未选择的视频将不会下载但会在数据库中记录。新发布的投稿会自动下载。
										</p>
									</div>
								{/if}
							</div>
						{/if}

						<!-- 名称 -->
						<div class="space-y-2">
							<Label for="name">名称</Label>
							<Input
								id="name"
								bind:value={name}
								placeholder="请输入视频源名称"
								required
							/>
						</div>

						<!-- 保存路径 -->
						<div class="space-y-2">
							<Label for="path">保存路径</Label>
							<Input
								id="path"
								bind:value={path}
								placeholder="例如：D:/Videos/Bilibili"
								required
							/>
							<p class="text-muted-foreground text-sm">请输入绝对路径</p>
						</div>

						<!-- 提交按钮 -->
						<div class="flex {isMobile ? 'flex-col' : ''} gap-2">
							<Button type="submit" disabled={loading} class={isMobile ? 'w-full' : ''}>
								{loading ? '添加中...' : '添加'}
							</Button>
							<Button
								type="button"
								variant="outline"
								onclick={() => goto('/')}
								class={isMobile ? 'w-full' : ''}
							>
								取消
							</Button>
						</div>
					</form>
				</div>

				<!-- 右侧：搜索结果区域 -->
				{#if showSearchResults && searchResults.length > 0}
					<div
						class={isMobile ? 'mt-6 w-full' : 'min-w-[550px] flex-1'}
						transition:fly={{ x: 300, duration: 300 }}
					>
						<div
							class="bg-card rounded-lg border {isMobile
								? ''
								: 'h-full'} flex flex-col overflow-hidden {isMobile
								? ''
								: 'sticky top-6'} max-h-[calc(100vh-200px)]"
						>
							<div class="bg-muted flex items-center justify-between border-b p-4">
								<div>
									<span class="text-foreground text-base font-medium">搜索结果</span>
									<span class="text-muted-foreground text-sm {isMobile ? 'block' : 'ml-2'}">
										共找到 {searchTotalResults} 个结果
									</span>
								</div>
								<div class="flex items-center gap-2">
									{#if batchMode && sourceType === 'submission'}
										<Button
											size="sm"
											variant="outline"
											onclick={() => selectAllVisible('search')}
											class="text-xs"
										>
											全选
										</Button>
									{/if}
									<button
										onclick={() => {
											showSearchResults = false;
											searchResults = [];
											searchTotalResults = 0;
										}}
										class="text-muted-foreground hover:text-foreground p-1 text-xl"
									>
										<X class="h-5 w-5" />
									</button>
								</div>
							</div>

							<div class="flex-1 overflow-hidden p-3">
								<div class="seasons-grid-container h-full">
									<div
										class="grid gap-4 {isMobile ? 'grid-cols-1' : ''}"
										style={isMobile
											? ''
											: 'grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));'}
									>
										{#each filteredSearchResults as result, i (result.bvid || result.season_id || result.mid || i)}
											{@const itemKey = `search_${result.bvid || result.season_id || result.mid || i}`}
											<button
												onclick={() => {
													if (batchMode && sourceType === 'submission') {
														toggleBatchSelection(itemKey, result, 'search');
													} else {
														selectSearchResult(result);
													}
												}}
												onmouseenter={(e) => handleMouseEnter(result, e)}
												onmouseleave={handleMouseLeave}
												onmousemove={handleMouseMove}
												class="hover:bg-muted relative flex transform items-start gap-3 rounded-lg border p-4 text-left transition-all duration-300 hover:scale-102 hover:shadow-md {batchMode && isBatchSelected(itemKey)
													? 'bg-blue-50 ring-2 ring-blue-500 dark:bg-blue-950'
													: ''}"
												transition:fly={{ y: 50, duration: 300, delay: i * 50 }}
												animate:flip={{ duration: 300 }}
											>
												<!-- 批量模式下的复选框 -->
												{#if batchMode && sourceType === 'submission'}
													<div class="flex-shrink-0 pt-1">
														<input
															type="checkbox"
															checked={batchCheckboxStates[itemKey] || false}
															onclick={(e) => {
																e.stopPropagation();
																toggleBatchSelection(itemKey, result, 'search');
															}}
															class="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
														/>
													</div>
												{/if}
												{#if result.cover}
													<img
														src={processBilibiliImageUrl(result.cover)}
														alt={result.title}
														class="h-14 w-20 flex-shrink-0 rounded object-cover"
														onerror={handleImageError}
														loading="lazy"
														crossorigin="anonymous"
														referrerpolicy="no-referrer"
													/>
												{:else}
													<div
														class="h-14 w-20 bg-muted text-muted-foreground flex flex-shrink-0 items-center justify-center rounded text-xs"
													>
														无图片
													</div>
												{/if}
												<div class="min-w-0 flex-1">
													<div class="mb-1 flex items-center gap-2">
														<h4 class="text-foreground flex-1 truncate text-sm font-medium">
															<!-- eslint-disable-next-line svelte/no-at-html-tags -->
															{@html result.title}
														</h4>
														{#if result.result_type}
															<span
																class="flex-shrink-0 rounded px-1.5 py-0.5 text-xs {result.result_type ===
																'media_bangumi'
																	? 'bg-purple-100 text-purple-700 dark:bg-purple-900 dark:text-purple-300'
																	: result.result_type === 'media_ft'
																		? 'bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300'
																		: result.result_type === 'bili_user'
																			? 'bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300'
																			: result.result_type === 'video'
																				? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
																				: 'text-foreground bg-gray-100 dark:bg-gray-800'}"
															>
																{result.result_type === 'media_bangumi'
																	? '番剧'
																	: result.result_type === 'media_ft'
																		? '影视'
																		: result.result_type === 'bili_user'
																			? 'UP主'
																			: result.result_type === 'video'
																				? '视频'
																				: result.result_type}
															</span>
														{/if}
														<!-- 显示已存在标记 -->
														{#if sourceType === 'submission' && result.mid && isSubmissionExists(Number(result.mid))}
															<span
																class="flex-shrink-0 rounded bg-gray-100 px-1.5 py-0.5 text-xs text-gray-700 dark:bg-gray-800 dark:text-gray-300"
															>
																已添加
															</span>
														{/if}
													</div>
													<p class="text-muted-foreground truncate text-xs">{result.author}</p>
													{#if result.description}
														<p class="text-muted-foreground/70 mt-1 line-clamp-2 text-xs">
															{result.description}
														</p>
													{/if}
												</div>
											</button>
										{/each}
									</div>
								</div>
							</div>

							{#if searchResults.length > 0}
								<div class="border-t p-3 text-center">
									<span class="text-muted-foreground text-xs">
										共显示 {searchResults.length} 个结果
										{#if searchTotalResults > searchResults.length}
											（总共 {searchTotalResults} 个）
										{/if}
									</span>
								</div>
							{/if}
						</div>
					</div>
				{/if}

				<!-- 关注UP主列表（移动到右侧） -->
				{#if (sourceType === 'collection' || sourceType === 'submission') && userFollowings.length > 0}
					<div class={isMobile ? 'mt-6 w-full' : 'flex-1'}>
						<div
							class="bg-card rounded-lg border {isMobile
								? ''
								: 'h-full'} flex flex-col overflow-hidden {isMobile
								? ''
								: 'sticky top-6'} max-h-126"
						>
							<div
								class="flex items-center justify-between border-b bg-blue-50 p-4 dark:bg-blue-950"
							>
								<div>
									<span class="text-base font-medium text-blue-800 dark:text-blue-200"
										>关注的UP主</span
									>
									<span
										class="text-sm text-blue-600 dark:text-blue-400 {isMobile ? 'block' : 'ml-2'}"
									>
										共 {userFollowings.length} 个UP主
									</span>
								</div>
								{#if batchMode}
									<Button
										size="sm"
										variant="outline"
										onclick={() => selectAllVisible('following')}
										class="text-xs"
									>
										全选
									</Button>
								{/if}
							</div>

							<div class="flex-1 overflow-y-auto p-3">
								<div
									class="grid gap-3 {isMobile ? 'grid-cols-1' : ''}"
									style={isMobile
										? ''
										: 'grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));'}
								>
									{#each filteredUserFollowings as following (following.mid)}
										{@const itemKey = `following_${following.mid}`}
										{@const isDisabled =
											sourceType === 'submission' && existingSubmissionIds.has(following.mid)}
										<button
											onclick={() => {
												if (batchMode) {
													toggleBatchSelection(itemKey, following, 'following');
												} else {
													selectFollowing(following);
												}
											}}
											disabled={isDisabled}
											class="hover:bg-muted rounded-lg border p-3 text-left transition-colors {isDisabled
												? 'cursor-not-allowed opacity-60'
												: ''} {batchMode && isBatchSelected(itemKey)
												? 'bg-blue-50 ring-2 ring-blue-500 dark:bg-blue-950'
												: ''}"
										>
											<div class="flex items-start gap-2">
												<!-- 批量模式下的复选框 -->
												{#if batchMode}
													<div class="flex-shrink-0 pt-1">
														<input
															type="checkbox"
															checked={batchCheckboxStates[itemKey] || false}
															onclick={(e) => {
																e.stopPropagation();
																toggleBatchSelection(itemKey, following, 'following');
															}}
															class="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
														/>
													</div>
												{/if}
												{#if following.face}
													<img
														src={processBilibiliImageUrl(following.face)}
														alt={following.name}
														class="h-10 w-10 flex-shrink-0 rounded-full object-cover"
														onerror={handleImageError}
														loading="lazy"
														crossorigin="anonymous"
														referrerpolicy="no-referrer"
													/>
												{:else}
													<div
														class="bg-muted text-muted-foreground flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-full text-xs"
													>
														头像
													</div>
												{/if}
												<div class="min-w-0 flex-1">
													<div class="mb-1 flex items-center gap-1">
														<h4 class="truncate text-xs font-medium">{following.name}</h4>
														{#if following.official_verify && following.official_verify.type >= 0}
															<span
																class="flex-shrink-0 rounded bg-yellow-100 px-1 py-0.5 text-xs text-yellow-700 dark:bg-yellow-900 dark:text-yellow-300"
															>
																V
															</span>
														{/if}
														{#if sourceType === 'submission' && existingSubmissionIds.has(following.mid)}
															<span
																class="flex-shrink-0 rounded bg-gray-100 px-1 py-0.5 text-xs text-gray-700 dark:bg-gray-800 dark:text-gray-300"
															>
																已添加
															</span>
														{/if}
													</div>
													<p class="text-muted-foreground mb-1 truncate text-xs">
														UID: {following.mid}
													</p>
													{#if following.sign}
														<p class="text-muted-foreground line-clamp-1 text-xs">
															{following.sign}
														</p>
													{/if}
												</div>
											</div>
										</button>
									{/each}
								</div>
							</div>
						</div>
					</div>
				{/if}

				<!-- UP主合集列表（移动到右侧） -->
				{#if sourceType === 'collection' && userCollections.length > 0}
					<div class={isMobile ? 'mt-6 w-full' : 'flex-1'}>
						<div
							class="bg-card rounded-lg border {isMobile
								? ''
								: 'h-full'} flex flex-col overflow-hidden {isMobile
								? ''
								: 'sticky top-6'} max-h-[calc(100vh-200px)]"
						>
							<div
								class="flex items-center justify-between border-b bg-green-50 p-4 dark:bg-green-950"
							>
								<div>
									<span class="text-base font-medium text-green-800 dark:text-green-200"
										>UP主合集列表</span
									>
									<span
										class="text-sm text-green-600 dark:text-green-400 {isMobile ? 'block' : 'ml-2'}"
									>
										共 {userCollections.length} 个合集
									</span>
								</div>
								{#if batchMode}
									<Button
										size="sm"
										variant="outline"
										onclick={() => selectAllVisible('collection')}
										class="text-xs"
									>
										全选
									</Button>
								{/if}
							</div>

							<div class="flex-1 overflow-y-auto p-3">
								<div
									class="grid gap-4 {isMobile ? 'grid-cols-1' : ''}"
									style={isMobile
										? ''
										: 'grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));'}
								>
									{#each filteredUserCollections as collection (collection.sid)}
										{@const itemKey = `collection_${collection.sid}`}
										{@const isDisabled = isCollectionExists(
											collection.sid,
											collection.mid.toString()
										)}
										<button
											onclick={() => {
												if (batchMode) {
													toggleBatchSelection(itemKey, collection, 'collection');
												} else {
													selectCollection(collection);
												}
											}}
											disabled={isDisabled}
											class="hover:bg-muted rounded-lg border p-4 text-left transition-colors {isDisabled
												? 'cursor-not-allowed opacity-60'
												: ''} {batchMode && isBatchSelected(itemKey)
												? 'bg-blue-50 ring-2 ring-blue-500 dark:bg-blue-950'
												: ''}"
										>
											<div class="flex items-start gap-3">
												<!-- 批量模式下的复选框 -->
												{#if batchMode}
													<div class="flex-shrink-0 pt-1">
														<input
															type="checkbox"
															checked={batchCheckboxStates[itemKey] || false}
															onclick={(e) => {
																e.stopPropagation();
																toggleBatchSelection(itemKey, collection, 'collection');
															}}
															class="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
														/>
													</div>
												{/if}
												{#if collection.cover}
													<img
														src={processBilibiliImageUrl(collection.cover)}
														alt={collection.name}
														class="h-16 w-24 flex-shrink-0 rounded object-cover"
														onerror={handleImageError}
														loading="lazy"
														crossorigin="anonymous"
														referrerpolicy="no-referrer"
													/>
												{:else}
													<div
														class="bg-muted text-muted-foreground flex h-16 w-24 flex-shrink-0 items-center justify-center rounded text-xs"
													>
														无封面
													</div>
												{/if}
												<div class="min-w-0 flex-1">
													<div class="mb-1 flex items-center gap-2">
														<h4 class="truncate text-sm font-medium">{collection.name}</h4>
														<span
															class="flex-shrink-0 rounded px-2 py-0.5 text-xs {collection.collection_type ===
															'season'
																? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
																: 'bg-blue-100 text-blue-700'}"
														>
															{collection.collection_type === 'season' ? '合集' : '系列'}
														</span>
														{#if isCollectionExists(collection.sid, collection.mid.toString())}
															<span
																class="flex-shrink-0 rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-700 dark:bg-gray-800 dark:text-gray-300"
															>
																已添加
															</span>
														{/if}
													</div>
													<p class="text-muted-foreground mb-1 text-xs">
														ID: {collection.sid} (检查key: {collection.sid}_{collection.mid})
													</p>
													<p class="text-muted-foreground text-xs">共 {collection.total} 个视频</p>
													{#if collection.description}
														<p class="text-muted-foreground mt-1 line-clamp-2 text-xs">
															{collection.description}
														</p>
													{/if}
												</div>
											</div>
										</button>
									{/each}
								</div>
							</div>
						</div>
					</div>
				{/if}

				<!-- 收藏夹列表（移动到右侧） -->
				{#if sourceType === 'favorite' && userFavorites.length > 0}
					<div class={isMobile ? 'mt-6 w-full' : 'flex-1'}>
						<div
							class="bg-card rounded-lg border {isMobile
								? ''
								: 'h-full'} flex flex-col overflow-hidden {isMobile
								? ''
								: 'sticky top-6'} max-h-[calc(100vh-200px)]"
						>
							<div
								class="flex items-center justify-between border-b bg-yellow-50 p-4 dark:bg-yellow-950"
							>
								<div>
									<span class="text-base font-medium text-yellow-800 dark:text-yellow-200"
										>我的收藏夹</span
									>
									<span
										class="text-sm text-yellow-600 dark:text-yellow-400 {isMobile
											? 'block'
											: 'ml-2'}"
									>
										共 {userFavorites.length} 个收藏夹
									</span>
								</div>
								{#if batchMode}
									<Button
										size="sm"
										variant="outline"
										onclick={() => selectAllVisible('favorite')}
										class="text-xs"
									>
										全选
									</Button>
								{/if}
							</div>

							<div class="flex-1 overflow-y-auto p-3">
								<div
									class="grid gap-4 {isMobile ? 'grid-cols-1' : ''}"
									style={isMobile
										? ''
										: 'grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));'}
								>
									{#each filteredUserFavorites as favorite (favorite.id)}
										{@const itemKey = `favorite_${favorite.id}`}
										{@const isDisabled = existingFavoriteIds.has(Number(favorite.id))}
										<button
											onclick={() => {
												if (batchMode) {
													toggleBatchSelection(itemKey, favorite, 'favorite');
												} else {
													selectFavorite(favorite);
												}
											}}
											disabled={isDisabled}
											class="hover:bg-muted rounded-lg border p-4 text-left transition-colors {isDisabled
												? 'cursor-not-allowed opacity-60'
												: ''} {batchMode && isBatchSelected(itemKey)
												? 'bg-blue-50 ring-2 ring-blue-500 dark:bg-blue-950'
												: ''}"
										>
											<div class="flex items-start gap-3">
												<!-- 批量模式下的复选框 -->
												{#if batchMode}
													<div class="flex-shrink-0 pt-1">
														<input
															type="checkbox"
															checked={batchCheckboxStates[itemKey] || false}
															onclick={(e) => {
																e.stopPropagation();
																toggleBatchSelection(itemKey, favorite, 'favorite');
															}}
															class="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
														/>
													</div>
												{/if}
												{#if favorite.cover}
													<img
														src={processBilibiliImageUrl(favorite.cover)}
														alt={favorite.name || favorite.title}
														class="h-16 w-24 flex-shrink-0 rounded object-cover"
														onerror={handleImageError}
														loading="lazy"
														crossorigin="anonymous"
														referrerpolicy="no-referrer"
													/>
												{:else}
													<div
														class="bg-muted text-muted-foreground flex h-16 w-24 flex-shrink-0 items-center justify-center rounded text-xs"
													>
														无封面
													</div>
												{/if}
												<div class="min-w-0 flex-1">
													<div class="mb-1 flex items-center gap-2">
														<h4 class="truncate text-sm font-medium">
															{favorite.name || favorite.title}
														</h4>
														{#if existingFavoriteIds.has(Number(favorite.id))}
															<span
																class="flex-shrink-0 rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-700 dark:bg-gray-800 dark:text-gray-300"
															>
																已添加
															</span>
														{/if}
													</div>
													<p class="text-muted-foreground mb-1 text-xs">收藏夹ID: {favorite.id}</p>
													<p class="text-muted-foreground mb-1 text-xs">
														共 {favorite.media_count} 个视频
													</p>
													{#if favorite.created}
														<p class="text-muted-foreground text-xs">
															创建于 {new Date(favorite.created * 1000).toLocaleDateString()}
														</p>
													{/if}
												</div>
											</div>
										</button>
									{/each}
								</div>
							</div>
						</div>
					</div>
				{/if}

				<!-- UP主收藏夹列表（移动到右侧） -->
				{#if sourceType === 'favorite' && selectedUserId && (searchedUserFavorites.length > 0 || loadingSearchedUserFavorites)}
					<div class={isMobile ? 'mt-6 w-full' : 'flex-1'}>
						<div
							class="bg-card rounded-lg border {isMobile
								? ''
								: 'h-full'} flex flex-col overflow-hidden {isMobile
								? ''
								: 'sticky top-6'} max-h-[calc(100vh-200px)]"
						>
							<div
								class="flex items-center justify-between border-b bg-green-50 p-4 dark:bg-green-950"
							>
								<div>
									<span class="text-base font-medium text-green-800 dark:text-green-200"
										>{selectedUserName} 的收藏夹</span
									>
									<span
										class="text-sm text-green-600 dark:text-green-400 {isMobile ? 'block' : 'ml-2'}"
									>
										{#if loadingSearchedUserFavorites}
											正在加载...
										{:else if searchedUserFavorites.length > 0}
											共 {searchedUserFavorites.length} 个收藏夹
										{:else}
											没有公开收藏夹
										{/if}
									</span>
								</div>
								<div class="flex items-center gap-2">
									{#if batchMode && searchedUserFavorites.length > 0}
										<Button
											size="sm"
											variant="outline"
											onclick={() => selectAllVisible('searched-favorite')}
											class="text-xs"
										>
											全选
										</Button>
									{/if}
									<button
										onclick={() => {
											selectedUserId = '';
											selectedUserName = '';
											searchedUserFavorites = [];
											loadingSearchedUserFavorites = false;
										}}
										class="p-1 text-xl text-green-500 hover:text-green-700 dark:text-green-300"
									>
										<X class="h-5 w-5" />
									</button>
								</div>
							</div>

							<div class="flex-1 overflow-y-auto p-3">
								{#if loadingSearchedUserFavorites}
									<div class="p-4 text-center">
										<div class="text-sm text-green-700 dark:text-green-300">
											正在获取收藏夹列表...
										</div>
									</div>
								{:else if searchedUserFavorites.length > 0}
									<div
										class="grid gap-4 {isMobile ? 'grid-cols-1' : ''}"
										style={isMobile
											? ''
											: 'grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));'}
									>
										{#each filteredSearchedUserFavorites as favorite (favorite.fid)}
											{@const itemKey = `searched-favorite_${favorite.fid}`}
												{@const isDisabled = existingFavoriteIds.has(Number(favorite.fid))}
											<button
												onclick={() => {
													if (batchMode) {
														toggleBatchSelection(itemKey, favorite, 'favorite');
													} else {
														selectSearchedFavorite(favorite);
													}
												}}
												disabled={isDisabled}
												class="hover:bg-muted rounded-lg border p-4 text-left transition-colors {isDisabled
													? 'cursor-not-allowed opacity-60'
													: ''} {batchMode && isBatchSelected(itemKey)
													? 'bg-blue-50 ring-2 ring-blue-500 dark:bg-blue-950'
													: ''}"
											>
												<div class="flex items-start gap-3">
													<!-- 批量模式下的复选框 -->
													{#if batchMode}
														<div class="flex-shrink-0 pt-1">
															<input
																type="checkbox"
																checked={batchCheckboxStates[itemKey] || false}
																onclick={(e) => {
																	e.stopPropagation();
																	toggleBatchSelection(itemKey, favorite, 'favorite');
																}}
																class="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
															/>
														</div>
													{/if}
													<div
														class="bg-muted text-muted-foreground flex h-16 w-24 flex-shrink-0 items-center justify-center rounded text-xs"
													>
														收藏夹
													</div>
													<div class="min-w-0 flex-1">
														<div class="mb-1 flex items-center gap-2">
															<h4 class="truncate text-sm font-medium">{favorite.title}</h4>
															{#if existingFavoriteIds.has(Number(favorite.fid))}
																<span
																	class="flex-shrink-0 rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-700 dark:bg-gray-800 dark:text-gray-300"
																>
																	已添加
																</span>
															{/if}
														</div>
														<p class="text-muted-foreground mb-1 text-xs">
															收藏夹ID: {favorite.fid}
														</p>
														<p class="text-muted-foreground text-xs">
															共 {favorite.media_count} 个视频
														</p>
													</div>
												</div>
											</button>
										{/each}
									</div>
								{:else}
									<div class="p-4 text-center">
										<div class="text-muted-foreground text-sm">该UP主没有公开的收藏夹</div>
									</div>
								{/if}
							</div>
						</div>
					</div>
				{/if}

				<!-- 订阅的合集列表（仅合集类型时显示） -->
				{#if sourceType === 'collection' && subscribedCollections.length > 0}
					<div class={isMobile ? 'mt-6 w-full' : 'flex-1'}>
						<div
							class="bg-card rounded-lg border {isMobile
								? ''
								: 'h-full'} flex flex-col overflow-hidden {isMobile ? '' : 'sticky top-6'} max-h-96"
						>
							<div
								class="flex items-center justify-between border-b bg-purple-50 p-4 dark:bg-purple-950"
							>
								<div>
									<span class="text-base font-medium text-purple-800 dark:text-purple-200"
										>关注的合集</span
									>
									<span
										class="text-sm text-purple-600 dark:text-purple-400 {isMobile
											? 'block'
											: 'ml-2'}"
									>
										共 {subscribedCollections.length} 个合集
									</span>
								</div>
								{#if batchMode}
									<Button
										size="sm"
										variant="outline"
										onclick={() => selectAllVisible('subscribed-collection')}
										class="text-xs"
									>
										全选
									</Button>
								{/if}
							</div>

							<div class="flex-1 overflow-y-auto p-3">
								<div
									class="grid gap-4 {isMobile ? 'grid-cols-1' : ''}"
									style={isMobile
										? ''
										: 'grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));'}
								>
									{#each subscribedCollections as collection (collection.sid)}
										{@const itemKey = `subscribed-collection_${collection.sid}`}
										{@const isExisting = isCollectionExists(
											collection.sid,
											collection.up_mid.toString()
										)}
										<button
											onclick={() => {
												if (batchMode) {
													toggleBatchSelection(itemKey, collection, 'collection');
												} else {
													selectSubscribedCollection(collection);
												}
											}}
											disabled={isExisting}
											class="hover:bg-muted rounded-lg border p-4 text-left transition-colors {isExisting
												? 'cursor-not-allowed opacity-60'
												: ''} {batchMode && isBatchSelected(itemKey)
												? 'bg-blue-50 ring-2 ring-blue-500 dark:bg-blue-950'
												: ''}"
										>
											<div class="flex items-start gap-3">
												<!-- 批量模式下的复选框 -->
												{#if batchMode}
													<div class="flex-shrink-0 pt-1">
														<input
															type="checkbox"
															checked={batchCheckboxStates[itemKey] || false}
															onclick={(e) => {
																e.stopPropagation();
																toggleBatchSelection(itemKey, collection, 'collection');
															}}
															class="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
														/>
													</div>
												{/if}
												{#if collection.cover}
													<img
														src={processBilibiliImageUrl(collection.cover)}
														alt={collection.name}
														class="h-16 w-24 flex-shrink-0 rounded object-cover"
														onerror={handleImageError}
														loading="lazy"
														crossorigin="anonymous"
														referrerpolicy="no-referrer"
													/>
												{:else}
													<div
														class="bg-muted text-muted-foreground flex h-16 w-24 flex-shrink-0 items-center justify-center rounded text-xs"
													>
														无封面
													</div>
												{/if}
												<div class="min-w-0 flex-1">
													<div class="mb-1 flex items-center gap-2">
														<h4 class="truncate text-sm font-medium">{collection.name}</h4>
														<span
															class="flex-shrink-0 rounded bg-purple-100 px-2 py-0.5 text-xs text-purple-700 dark:bg-purple-900 dark:text-purple-300"
														>
															{collection.collection_type === 'season' ? '合集' : '系列'}
														</span>
														{#if isExisting}
															<span
																class="flex-shrink-0 rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-700 dark:bg-gray-800 dark:text-gray-300"
															>
																已添加
															</span>
														{/if}
													</div>
													<p class="text-muted-foreground mb-1 text-xs">ID: {collection.sid}</p>
													<p class="text-muted-foreground mb-1 text-xs">
														UP主: {collection.up_name}
													</p>
													<p class="text-muted-foreground text-xs">共 {collection.total} 个视频</p>
													{#if collection.description}
														<p class="text-muted-foreground mt-1 line-clamp-2 text-xs">
															{collection.description}
														</p>
													{/if}
												</div>
											</div>
										</button>
									{/each}
								</div>
							</div>
						</div>
					</div>
				{/if}

				<!-- UP主投稿选择面板（仅投稿类型时显示） -->
				{#if sourceType === 'submission' && showSubmissionSelection}
					<div
						class={isMobile ? 'mt-6 w-full' : 'flex-1'}
						transition:fly={{ x: 300, duration: 300 }}
					>
						<div
							class="bg-card rounded-lg border {isMobile
								? ''
								: 'h-full'} flex flex-col overflow-hidden {isMobile
								? ''
								: 'sticky top-6'} max-h-[750px]"
						>
							<div
								class="flex items-center justify-between border-b bg-blue-50 p-4 dark:bg-blue-950"
							>
								<div>
									<div class="flex items-center gap-2">
										<span class="text-base font-medium text-blue-800 dark:text-blue-200"
											>📹 选择历史投稿</span
										>
										<span class="text-xs text-blue-600 dark:text-blue-400"
											>选择您希望下载的历史投稿。未选择的视频不会下载和显示。新发布的投稿会自动下载。</span
										>
									</div>
									<span
										class="text-sm text-blue-600 dark:text-blue-400 {isMobile
											? 'block'
											: 'ml-2'} mt-1"
									>
										{#if submissionLoading && submissionVideos.length === 0}
											正在加载...
										{:else if submissionTotalCount > 0}
											共 {submissionTotalCount} 个投稿
										{:else}
											暂无投稿
										{/if}
									</span>
								</div>
								<button
									onclick={cancelSubmissionSelection}
									class="p-1 text-xl text-blue-500 hover:text-blue-700 dark:text-blue-300 dark:hover:text-blue-100"
								>
									<X class="h-5 w-5" />
								</button>
							</div>

							<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
								{#if submissionError}
									<div class="m-3 rounded-lg border border-red-200 bg-red-50 p-4">
										<div class="flex items-center gap-2">
											<svg
												class="h-5 w-5 text-red-600"
												fill="none"
												stroke="currentColor"
												viewBox="0 0 24 24"
											>
												<path
													stroke-linecap="round"
													stroke-linejoin="round"
													stroke-width="2"
													d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
												/>
											</svg>
											<span class="text-sm font-medium text-red-800 dark:text-red-200"
												>加载失败</span
											>
										</div>
										<p class="mt-1 text-sm text-red-700 dark:text-red-300">{submissionError}</p>
										<button
											type="button"
											class="mt-2 text-sm text-red-600 underline hover:text-red-800 dark:text-red-400 dark:hover:text-red-200"
											onclick={loadSubmissionVideos}
										>
											重试
										</button>
									</div>
								{:else}
									<!-- 搜索和操作栏 -->
									<div class="flex-shrink-0 space-y-3 p-3">
										<div class="flex gap-2">
											<div class="relative flex-1">
												<input
													type="text"
													bind:value={submissionSearchQuery}
													placeholder="搜索视频标题（支持关键词搜索UP主所有视频）..."
													class="w-full rounded-md border border-gray-300 px-3 py-2 pr-8 text-sm focus:border-blue-500 focus:ring-2 focus:ring-blue-500 focus:outline-none"
													disabled={isSearching}
												/>
												{#if isSearching}
													<div class="absolute inset-y-0 right-0 flex items-center pr-3">
														<svg
															class="h-4 w-4 animate-spin text-blue-600"
															fill="none"
															viewBox="0 0 24 24"
														>
															<circle
																class="opacity-25"
																cx="12"
																cy="12"
																r="10"
																stroke="currentColor"
																stroke-width="4"
															></circle>
															<path
																class="opacity-75"
																fill="currentColor"
																d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
															></path>
														</svg>
													</div>
												{/if}
											</div>
										</div>

										{#if submissionSearchQuery.trim()}
											<div class="px-1 text-xs text-blue-600">
												{isSearching
													? '搜索中...'
													: `搜索模式：在UP主所有视频中搜索 "${submissionSearchQuery}"`}
											</div>
										{/if}

										<div class="flex items-center justify-between">
											<div class="flex gap-2">
												<button
													type="button"
													class="bg-card text-foreground hover:bg-muted rounded-md border border-gray-300 px-3 py-1 text-sm font-medium"
													onclick={selectAllSubmissions}
													disabled={filteredSubmissionVideos.length === 0}
												>
													全选
												</button>
												<button
													type="button"
													class="bg-card text-foreground hover:bg-muted rounded-md border border-gray-300 px-3 py-1 text-sm font-medium"
													onclick={selectNoneSubmissions}
													disabled={selectedSubmissionCount === 0}
												>
													全不选
												</button>
												<button
													type="button"
													class="bg-card text-foreground hover:bg-muted rounded-md border border-gray-300 px-3 py-1 text-sm font-medium"
													onclick={invertSubmissionSelection}
													disabled={filteredSubmissionVideos.length === 0}
												>
													反选
												</button>
											</div>

											<!-- 已选择视频计数器 - 可点击加载所有视频 -->
											<button
												type="button"
												class="cursor-pointer rounded px-2 py-1 text-sm font-medium transition-colors {submissionVideos.length >=
												submissionTotalCount
													? 'text-green-600 hover:bg-green-50 dark:text-green-400 dark:hover:bg-green-950'
													: 'text-orange-600 hover:bg-orange-50 dark:text-orange-400 dark:hover:bg-orange-950'}"
												onclick={loadAllRemainingVideos}
												disabled={isLoadingMore ||
													submissionVideos.length >= submissionTotalCount}
												title={submissionVideos.length >= submissionTotalCount
													? '已加载全部视频'
													: `点击加载剩余 ${submissionTotalCount - submissionVideos.length} 个视频`}
											>
												已选择 {selectedSubmissionCount} / {filteredSubmissionVideos.length} 个视频
												{#if submissionTotalCount > 0 && submissionVideos.length < submissionTotalCount}
													<span class="ml-1 text-xs">
														({submissionVideos.length}/{submissionTotalCount})
													</span>
												{/if}
											</button>
										</div>
									</div>

									<!-- 视频列表 -->
									<div
										class="min-h-0 flex-1 overflow-y-auto p-3 pt-0"
										bind:this={submissionScrollContainer}
										onscroll={handleSubmissionScroll}
									>
										{#if submissionLoading && submissionVideos.length === 0}
											<div class="flex items-center justify-center py-8">
												<svg
													class="h-8 w-8 animate-spin text-blue-600 dark:text-blue-400"
													fill="none"
													viewBox="0 0 24 24"
												>
													<circle
														class="opacity-25"
														cx="12"
														cy="12"
														r="10"
														stroke="currentColor"
														stroke-width="4"
													></circle>
													<path
														class="opacity-75"
														fill="currentColor"
														d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
													></path>
												</svg>
												<span class="text-muted-foreground ml-2 text-sm">加载中...</span>
											</div>
										{:else if filteredSubmissionVideos.length === 0}
											<div
												class="text-muted-foreground flex flex-col items-center justify-center py-8"
											>
												<svg
													class="mb-2 h-12 w-12"
													fill="none"
													stroke="currentColor"
													viewBox="0 0 24 24"
												>
													<path
														stroke-linecap="round"
														stroke-linejoin="round"
														stroke-width="2"
														d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"
													/>
												</svg>
												<p class="text-sm">没有找到视频</p>
											</div>
										{:else}
											<div
												class="grid gap-4 {isMobile ? 'grid-cols-1' : ''}"
												style={isMobile
													? ''
													: 'grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));'}
											>
												{#each filteredSubmissionVideos as video (video.bvid)}
													<div
														class="hover:bg-muted relative rounded-lg border p-4 transition-all duration-300 hover:shadow-md {selectedSubmissionVideos.has(
															video.bvid
														)
															? 'border-blue-300 bg-blue-50'
															: 'border-gray-200'} {isMobile ? 'h-auto' : 'h-[100px]'}"
													>
														<div class="flex h-full gap-3">
															<div class="relative flex-shrink-0">
																<img
																	src={processBilibiliImageUrl(video.cover)}
																	alt={video.title}
																	class="h-[63px] w-28 rounded object-cover"
																	loading="lazy"
																	crossorigin="anonymous"
																	referrerpolicy="no-referrer"
																	onerror={handleImageError}
																/>
															</div>
															<div class="relative flex min-w-0 flex-1 flex-col overflow-hidden">
																<input
																	type="checkbox"
																	checked={selectedSubmissionVideos.has(video.bvid)}
																	onchange={() => toggleSubmissionVideo(video.bvid)}
																	class="absolute top-1 right-1 z-10 h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500 dark:text-blue-400"
																/>
																<h4
																	class="mb-1 line-clamp-2 flex-shrink-0 pr-6 text-sm font-medium text-gray-900"
																>
																	{video.title}
																</h4>
																<p
																	class="text-muted-foreground mb-2 line-clamp-1 flex-shrink-0 text-xs"
																>
																	{video.description || '无简介'}
																</p>
																<div class="text-muted-foreground mt-auto text-xs">
																	<div class="flex flex-wrap items-center gap-2">
																		<span>🎬 {formatSubmissionPlayCount(video.view)}</span>
																		<span>💬 {formatSubmissionPlayCount(video.danmaku)}</span>
																		<span>📅 {formatSubmissionDate(video.pubtime)}</span>
																		<span class="font-mono text-xs">{video.bvid}</span>
																	</div>
																</div>
															</div>
														</div>
													</div>
												{/each}
											</div>

											{#if submissionVideos.length > 0}
												{#if showLoadMoreButton && hasMoreVideos}
													<div class="py-4 text-center">
														<button
															type="button"
															class="rounded-md border border-transparent bg-blue-600 px-6 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
															onclick={loadMoreSubmissionVideos}
															disabled={isLoadingMore}
														>
															{#if isLoadingMore}
																<div class="flex items-center gap-2">
																	<div
																		class="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent"
																	></div>
																	<span>加载中...</span>
																</div>
															{:else}
																加载更多 ({submissionVideos.length}/{submissionTotalCount})
															{/if}
														</button>
													</div>
												{:else if submissionTotalCount > 0}
													<div class="text-muted-foreground py-4 text-center text-sm">
														已加载全部 {submissionTotalCount} 个视频
													</div>
												{/if}
											{/if}
										{/if}
									</div>

									<!-- 确认按钮 -->
									<div class="flex flex-shrink-0 justify-end gap-3 border-t p-4">
										<button
											type="button"
											class="bg-card text-foreground hover:bg-muted rounded-md border border-gray-300 px-4 py-2 text-sm font-medium focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 focus:outline-none"
											onclick={cancelSubmissionSelection}
										>
											取消
										</button>
										<button
											type="button"
											class="rounded-md border border-transparent bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:outline-none"
											onclick={confirmSubmissionSelection}
										>
											确认选择 ({selectedSubmissionVideos.size} 个视频)
										</button>
									</div>
								{/if}
							</div>
						</div>
					</div>
				{/if}
			</div>
		</div>
	</div>
</div>

<!-- 批量操作工具栏 -->
{#if batchMode && batchSelectedItems.size > 0}
	<div
		class="fixed {isMobile
			? 'right-4 bottom-4 left-4'
			: 'bottom-6 left-1/2 -translate-x-1/2'} z-50 rounded-lg border border-blue-500 bg-blue-600 px-4 py-3 text-white shadow-xl transition-all duration-300 dark:border-blue-600 dark:bg-blue-700"
		transition:fly={{ y: 100, duration: 300 }}
	>
		<div class="flex {isMobile ? 'flex-col gap-3' : 'items-center gap-4'}">
			<div class="text-sm font-medium {isMobile ? 'text-center' : ''}">
				已选择 {batchSelectedItems.size} 个视频源
			</div>
			<div class="flex gap-2 {isMobile ? 'justify-center' : ''}">
				<Button
					size="sm"
					variant="secondary"
					onclick={clearBatchSelection}
					class="border-white/30 bg-white/20 text-xs text-white hover:bg-white/30"
				>
					清空
				</Button>
				<Button
					size="sm"
					variant="secondary"
					onclick={() => {
						batchDialogOpen = true;
					}}
					disabled={batchAdding}
					class="bg-white text-xs text-blue-600 hover:bg-gray-100 dark:text-blue-700"
				>
					{batchAdding ? '添加中...' : '批量添加'}
				</Button>
			</div>
		</div>
		{#if batchAdding}
			<div class="mt-2 text-xs text-blue-100 dark:text-blue-200 {isMobile ? 'text-center' : ''}">
				正在添加 ({batchProgress.current}/{batchProgress.total})
			</div>
		{/if}
	</div>
{/if}

<!-- 统一的悬停详情框 -->
{#if hoveredItem}
	<div
		class="bg-card pointer-events-none fixed z-50 max-w-md rounded-lg border p-4 shadow-2xl transition-all duration-150 ease-out"
		style="left: {mousePosition.x}px; top: {mousePosition.y}px;"
		transition:fade={{ duration: 200 }}
	>
		{#if hoveredItem.type === 'search'}
			<!-- 搜索结果详情内容 -->
			<div class="flex gap-4">
				{#if hoveredItem.data.cover}
					<img
						src={processBilibiliImageUrl(hoveredItem.data.cover)}
						alt={hoveredItem.data.title}
						class="h-20 w-32 flex-shrink-0 rounded object-cover"
						loading="lazy"
						crossorigin="anonymous"
						referrerpolicy="no-referrer"
					/>
				{:else}
					<div
						class="h-20 w-32 bg-muted text-muted-foreground flex flex-shrink-0 items-center justify-center rounded text-sm"
					>
						无图片
					</div>
				{/if}
				<div class="min-w-0 flex-1">
					<div class="mb-1 flex items-center gap-2">
						<h4 class="flex-1 text-sm font-semibold">
							<!-- eslint-disable-next-line svelte/no-at-html-tags -->
							{@html hoveredItem.data.title}
						</h4>
						{#if hoveredItem.data.result_type}
							<span
								class="flex-shrink-0 rounded px-1.5 py-0.5 text-xs {hoveredItem.data.result_type ===
								'media_bangumi'
									? 'bg-purple-100 text-purple-700 dark:bg-purple-900 dark:text-purple-300'
									: hoveredItem.data.result_type === 'media_ft'
										? 'bg-red-100 text-red-700'
										: hoveredItem.data.result_type === 'bili_user'
											? 'bg-blue-100 text-blue-700'
											: hoveredItem.data.result_type === 'video'
												? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
												: 'text-foreground bg-gray-100'}"
							>
								{hoveredItem.data.result_type === 'media_bangumi'
									? '番剧'
									: hoveredItem.data.result_type === 'media_ft'
										? '影视'
										: hoveredItem.data.result_type === 'bili_user'
											? 'UP主'
											: hoveredItem.data.result_type === 'video'
												? '视频'
												: hoveredItem.data.result_type}
							</span>
						{/if}
					</div>
					<p class="text-muted-foreground mb-2 text-xs">作者：{hoveredItem.data.author}</p>
					{#if hoveredItem.data.description}
						<p class="text-muted-foreground mb-2 line-clamp-4 text-xs">
							{hoveredItem.data.description}
						</p>
					{/if}
					<div class="flex flex-wrap gap-2 text-xs">
						{#if hoveredItem.data.play}
							<span class="text-muted-foreground flex items-center gap-1">
								<span>▶</span> 播放：{hoveredItem.data.play > 10000
									? (hoveredItem.data.play / 10000).toFixed(1) + '万'
									: hoveredItem.data.play}
							</span>
						{/if}
						{#if hoveredItem.data.danmaku}
							<span class="text-muted-foreground flex items-center gap-1">
								<span>💬</span> 弹幕：{hoveredItem.data.danmaku > 10000
									? (hoveredItem.data.danmaku / 10000).toFixed(1) + '万'
									: hoveredItem.data.danmaku}
							</span>
						{/if}
						{#if hoveredItem.data.bvid}
							<span class="text-muted-foreground">BV号: {hoveredItem.data.bvid}</span>
						{/if}
					</div>
				</div>
			</div>
		{/if}
	</div>
{/if}

<!-- 批量添加配置对话框 -->
{#if batchDialogOpen}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
		transition:fade
	>
		<div
			class="bg-card mx-4 w-full max-w-md rounded-lg border shadow-lg"
			transition:fly={{ y: -50 }}
		>
			<div class="border-b p-4">
				<h3 class="text-lg font-semibold">批量添加配置</h3>
				<p class="text-muted-foreground mt-1 text-sm">
					将添加 {batchSelectedItems.size} 个视频源
				</p>
			</div>

			<div class="space-y-4 p-4">
				<div>
					<Label for="batch-base-path">基础保存路径</Label>
					<Input
						id="batch-base-path"
						bind:value={batchBasePath}
						placeholder="/Downloads"
						class="mt-1"
					/>
					<p class="text-muted-foreground mt-1 text-xs">所有选中的视频源将保存到此路径</p>
				</div>

				<div class="max-h-60 overflow-y-auto rounded border">
					<div class="space-y-2 p-3">
						{#each Array.from(batchSelectedItems.values()) as item, index}
							<div class="bg-muted flex items-center justify-between rounded p-2 text-sm">
								<div class="min-w-0 flex-1">
									<div class="truncate font-medium">{item.name}</div>
									<div class="text-muted-foreground truncate text-xs">
										{batchBasePath}
									</div>
								</div>
								<span class="bg-background ml-2 rounded px-2 py-1 text-xs">
									{item.type === 'search'
										? '搜索'
										: item.type === 'favorite'
											? '收藏夹'
											: item.type === 'following'
												? 'UP主'
												: item.type}
								</span>
							</div>
						{/each}
					</div>
				</div>
			</div>

			<div class="flex justify-end gap-2 border-t p-4">
				<Button
					variant="outline"
					onclick={() => {
						batchDialogOpen = false;
					}}
					disabled={batchAdding}
				>
					取消
				</Button>
				<Button onclick={handleBatchAdd} disabled={batchAdding || !batchBasePath.trim()}>
					{batchAdding ? '添加中...' : '开始添加'}
				</Button>
			</div>
		</div>
	</div>
{/if}

<style>
	/* 确保图片加载失败时的占位符正确显示 */
	:global(.placeholder) {
		flex-shrink: 0;
	}

	/* 限制描述文字的行数 */
	.line-clamp-2 {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.line-clamp-3 {
		display: -webkit-box;
		-webkit-line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.line-clamp-4 {
		display: -webkit-box;
		-webkit-line-clamp: 4;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	/* 悬停动画效果 */
	.hover\:scale-102:hover {
		transform: scale(1.02);
	}

	.transform {
		transition:
			transform 0.3s ease,
			box-shadow 0.3s ease;
	}

	/* 季度网格容器滚动样式 */
	.seasons-grid-container {
		max-height: calc(120px * 5 + 1rem * 4); /* 5个横向行，每行120px高度，4个行间隔 */
		overflow-y: auto;
		padding-right: 0.5rem;
	}

	.seasons-grid-container::-webkit-scrollbar {
		width: 6px;
	}

	.seasons-grid-container::-webkit-scrollbar-track {
		background: #f1f1f1;
		border-radius: 3px;
	}

	.seasons-grid-container::-webkit-scrollbar-thumb {
		background: #c1c1c1;
		border-radius: 3px;
	}

	.seasons-grid-container::-webkit-scrollbar-thumb:hover {
		background: #a1a1a1;
	}
</style>
