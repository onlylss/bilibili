import { writable } from 'svelte/store';
import { type VideoSourcesResponse, type VideoSource } from '$lib/types';

export const videoSourceStore = writable<VideoSourcesResponse | undefined>(undefined);

// 便捷的设置和清除方法
export const setVideoSources = (sources: VideoSourcesResponse) => {
	videoSourceStore.set(sources);
};

export const clearFilter = () => {
	videoSourceStore.set(undefined);
};

// 更新单个视频源的属性
export const updateVideoSource = (
	sourceType: string,
	sourceId: number,
	updates: Partial<VideoSource>
) => {
	videoSourceStore.update((current) => {
		if (!current) return current;

		// 创建新的对象以触发响应式更新
		const updated = { ...current };
		const typeKey = sourceType as keyof VideoSourcesResponse;

		if (updated[typeKey]) {
			updated[typeKey] = updated[typeKey].map((source) =>
				source.id === sourceId ? { ...source, ...updates } : source
			);
		}

		return updated;
	});
};

// 删除单个视频源
export const removeVideoSource = (sourceType: string, sourceId: number) => {
	videoSourceStore.update((current) => {
		if (!current) return current;

		const updated = { ...current };
		const typeKey = sourceType as keyof VideoSourcesResponse;

		if (updated[typeKey]) {
			updated[typeKey] = updated[typeKey].filter((source) => source.id !== sourceId);
		}

		return updated;
	});
};
