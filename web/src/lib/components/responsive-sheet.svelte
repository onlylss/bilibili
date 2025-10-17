<script lang="ts">
	import {
		Sheet,
		SheetContent,
		SheetHeader,
		SheetTitle,
		SheetDescription
	} from '$lib/components/ui/sheet';

	export let open: boolean = false;
	export let onOpenChange: (open: boolean) => void;
	export let title: string;
	export let description: string = '';
	export let isMobile: boolean = false;
	export let backgroundImage: string = '';

	let className: string = '';
	export { className as class };
</script>

<Sheet {open} {onOpenChange}>
	<SheetContent
		side={isMobile ? 'bottom' : 'right'}
		class="{isMobile
			? 'h-[95vh] max-h-[95vh] w-full max-w-none overflow-hidden rounded-t-lg'
			: '!inset-y-0 !right-0 !h-screen !w-screen !max-w-none'} [&>button]:hidden {className}"
	>
		{#if !isMobile && backgroundImage}
			<!-- 桌面端背景图 -->
			<div class="absolute inset-0 z-0 overflow-hidden">
				<img
					src={backgroundImage}
					alt="背景"
					class="h-full w-full object-cover"
					style="opacity: 0.6; filter: contrast(1.1) brightness(0.9);"
					loading="lazy"
				/>
				<div
					class="absolute inset-0"
					style="background: linear-gradient(to bottom right, rgba(255,255,255,0.85), rgba(255,255,255,0.5));"
				></div>
			</div>
		{/if}
		<div class="flex h-full items-center justify-center {isMobile ? '' : 'p-8'} relative z-10">
			<div
				class="{isMobile
					? 'bg-background h-full w-full max-w-none rounded-t-lg'
					: 'bg-card/95 w-full max-w-4xl rounded-lg border shadow-2xl backdrop-blur-sm'} relative overflow-hidden"
			>
				<SheetHeader
					class="{isMobile
						? 'bg-background/95 sticky top-0 z-20 border-b p-4 backdrop-blur-sm'
						: 'border-b p-6'} relative"
				>
					<SheetTitle class={isMobile ? 'text-lg' : 'text-xl'}>{title}</SheetTitle>
					{#if description}
						<SheetDescription class={isMobile ? 'text-sm' : ''}>{description}</SheetDescription>
					{/if}
					<!-- 自定义关闭按钮 -->
					<button
						onclick={() => onOpenChange(false)}
						class="ring-offset-background focus:ring-ring absolute {isMobile
							? 'top-3 right-3'
							: 'top-2 right-2'} rounded-sm p-1 opacity-70 transition-opacity hover:bg-gray-100 hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-none disabled:pointer-events-none"
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
				<slot />
			</div>
		</div>
	</SheetContent>
</Sheet>
