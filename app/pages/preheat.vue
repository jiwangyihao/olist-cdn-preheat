<template>
	<UDashboardGroup>
		<!-- Left Sidebar: Stats -->
		<UDashboardSidebar
			collapsible
			class="w-80 border-r border-gray-200 dark:border-gray-800"
			:ui="{ header: 'p-0' }"
		>
			<template #header>
				<UDashboardNavbar title="预热控制台" class="w-full">
					<template #right>
						<UBadge v-if="running" color="primary" variant="subtle" size="xs" class="animate-pulse">
							运行中
						</UBadge>
						<UBadge v-else-if="stopping" color="error" variant="subtle" size="xs" class="animate-pulse">
							停止中
						</UBadge>
						<UBadge v-else-if="crawling" color="warning" variant="subtle" size="xs" class="animate-pulse">
							扫描中
						</UBadge>
						<UButton
							v-if="running || stopping"
							:loading="stopping"
							:disabled="stopping"
							color="error"
							variant="soft"
							icon="i-heroicons-stop"
							size="sm"
							@click="cancelRun"
						>
							{{ stopping ? '停止中' : '停止' }}
						</UButton>
						<UButton
							v-else
							:loading="crawling"
							:color="canStart ? 'primary' : 'neutral'"
							:variant="canStart ? 'solid' : 'outline'"
							:icon="canStart ? 'i-heroicons-play' : 'i-heroicons-magnifying-glass'"
							size="sm"
							@click="handleAction"
						>
							{{ canStart ? '开始' : '扫描' }}
						</UButton>
					</template>
				</UDashboardNavbar>
			</template>

			<div class="flex flex-col h-full min-h-0">
				<!-- Top Section: Fixed -->
				<div class="p-4 space-y-4 shrink-0">
					<!-- Activity Status (Unified) -->
					<UCard v-if="running || crawling || stopping || stats.total > 0">
						<div class="space-y-4">
							<div class="flex items-center justify-between gap-2">
								<div class="text-sm font-medium text-gray-900 dark:text-white whitespace-nowrap">
									{{ crawling ? '扫描进度' : '预热进度' }}
								</div>
								<div v-if="running" class="text-xs font-mono text-primary-500 tabular-nums whitespace-nowrap">
									{{ formatSpeed(backendSpeed ?? globalStats.speed) }}
								</div>
								<div v-else-if="crawling" class="text-xs font-mono text-warning-500 tabular-nums whitespace-nowrap">
									{{ crawlStats.speed < 10 ? crawlStats.speed.toFixed(1) : Math.round(crawlStats.speed) }} it/s
								</div>
							</div>

							<!-- Progress Bar -->
							<div class="space-y-2">
								<UProgress
									:model-value="crawling ? (crawlStats.total_dirs > 0 ? (crawlStats.scanned_dirs / crawlStats.total_dirs) * 100 : 0) : globalStats.progress"
									size="md"
									:color="crawling ? 'warning' : 'primary'"
								/>
								<div class="flex justify-between text-[10px] text-gray-500 tabular-nums">
									<span v-if="crawling" class="truncate mr-2">{{ crawlStats.scanned_dirs }} / {{ crawlStats.total_dirs }} 目录</span>
									<span v-else class="truncate mr-2">{{ formatBytes(globalStats.effectiveRead) }} / {{ formatBytes(globalStats.totalSize) }}</span>

									<span v-if="crawling" class="shrink-0">{{ crawlStats.total_dirs > 0 ? Math.round((crawlStats.scanned_dirs / crawlStats.total_dirs) * 100) : 0 }}%</span>
									<span v-else class="shrink-0">{{ Math.round(globalStats.progress) }}%</span>
								</div>
							</div>

							<!-- Details -->
							<div v-if="crawling" class="space-y-2">
								<UTooltip :text="crawlStats.current_path" :popper="{ placement: 'top' }" class="w-full">
									<div class="text-[10px] text-gray-500 truncate cursor-help bg-gray-50 dark:bg-gray-800/50 p-1.5 rounded border border-gray-100 dark:border-gray-800">
										{{ crawlStats.current_path }}
									</div>
								</UTooltip>
								<div class="flex justify-between text-[10px] text-gray-500 tabular-nums">
									<span>已发现 {{ crawlStats.found_files }} 文件</span>
									<span class="font-medium text-gray-900 dark:text-white">剩余 {{ formatDuration(crawlStats.eta) }}</span>
								</div>
							</div>
							<div v-else-if="running" class="flex justify-between text-[10px] text-gray-500 tabular-nums pt-2 border-t border-gray-100 dark:border-gray-800">
								<span>剩余时间</span>
								<span class="font-medium text-gray-900 dark:text-white">{{ formatDuration(globalStats.eta) }}</span>
							</div>
						</div>
					</UCard>

					<!-- Stats List (Unified Card) -->
					<UCard :ui="{ body: 'p-0!' }">
						<div class="flex divide-x divide-gray-100 dark:divide-gray-800">
							<div class="flex-1 flex flex-col items-center justify-center py-3 px-2">
								<div class="text-xs text-gray-500 font-medium mb-0.5 whitespace-nowrap">
									总文件
								</div>
								<div class="text-base font-bold text-gray-900 dark:text-white tabular-nums">
									{{ crawling ? crawlStats.found_files : stats.total }}
								</div>
							</div>
							<div class="flex-1 flex flex-col items-center justify-center py-3 px-2">
								<div class="text-xs text-gray-500 font-medium mb-0.5 whitespace-nowrap">
									已完成
								</div>
								<div class="text-base font-bold text-green-500 tabular-nums">
									{{ stats.done }}
								</div>
							</div>
							<div class="flex-1 flex flex-col items-center justify-center py-3 px-2">
								<div class="text-xs text-gray-500 font-medium mb-0.5 whitespace-nowrap">
									失败
								</div>
								<div class="text-base font-bold text-red-500 tabular-nums">
									{{ stats.failed }}
								</div>
							</div>
							<div class="flex-1 flex flex-col items-center justify-center py-3 px-2">
								<div class="text-xs text-gray-500 font-medium mb-0.5 whitespace-nowrap">
									运行中
								</div>
								<div class="text-base font-bold text-primary-500 tabular-nums">
									{{ globalStats.activeCount }}
								</div>
							</div>
						</div>
					</UCard>
				</div>

				<!-- Bottom Section: Scrollable Settings -->
				<div class="p-4 pt-0 flex-1 min-h-0 flex flex-col">
					<!-- Settings -->
					<UCard v-if="siteSettings" class="flex flex-col flex-1 min-h-0" :ui="{ body: 'overflow-y-auto flex-1 min-h-0' }">
						<template #header>
							<div class="text-sm font-bold">
								设置
							</div>
						</template>
						<div class="space-y-3">
							<UFormField label="API 地址">
								<UInput v-model="siteSettings.apiBaseUrl" />
							</UFormField>
							<UFormField label="Token">
								<UInput v-model="siteSettings.token" type="password" />
							</UFormField>
							<UFormField label="起始路径">
								<UInput v-model="siteSettings.startPath" />
							</UFormField>
							<UAccordion :items="[{ label: '运行设置', slot: 'run-settings' }, { label: '高级设置', slot: 'advanced' }]">
								<template #run-settings>
									<div class="space-y-3 pt-2">
										<UFormField label="最大并发数">
											<UInput v-model.number="runSettings.maxConn" type="number" />
										</UFormField>
										<UFormField label="速度限制 (MB/s, 0为不限)">
											<UInput v-model.number="rateLimitMB" type="number" />
										</UFormField>
									</div>
								</template>
								<template #advanced>
									<div class="space-y-3 pt-2">
										<UFormField label="代理地址">
											<UInput v-model="siteSettings.proxyUrl" placeholder="http://..." />
										</UFormField>
										<UFormField label="User Agent">
											<UInput v-model="siteSettings.userAgent" />
										</UFormField>
										<UFormField label="Cookie">
											<UTextarea v-model="siteSettings.cookie" :rows="2" />
										</UFormField>
									</div>
								</template>
							</UAccordion>
						</div>
					</UCard>
				</div>
			</div>
		</UDashboardSidebar>

		<!-- Right Content: File List -->
		<UDashboardPanel>
			<div class="relative flex items-center justify-center py-4 shrink-0">
				<UTabs
					v-model="selectedTab"
					:items="tabItems"
					size="xs"
					:ui="{ root: 'gap-0!' }"
				/>
				<div class="absolute right-4 top-1/2 -translate-y-1/2 flex items-center gap-2">
					<UTooltip text="复制当前列表链接">
						<UButton
							icon="i-heroicons-clipboard-document-list"
							color="neutral"
							variant="ghost"
							size="xs"
							@click="copyAllLinks"
						/>
					</UTooltip>
					<UTooltip text="重试当前列表失败任务">
						<UButton
							icon="i-heroicons-arrow-path"
							color="neutral"
							variant="ghost"
							size="xs"
							@click="retryAll"
						/>
					</UTooltip>
				</div>
			</div>

			<!-- Scanning State -->
			<div v-if="crawling" class="flex-1 flex flex-col items-center justify-center space-y-8 bg-white dark:bg-gray-900">
				<div class="relative w-32 h-32 flex items-center justify-center">
					<!-- Outer Ring -->
					<div class="absolute inset-0 border-4 border-primary-100 dark:border-primary-900/30 rounded-full" />
					<!-- Spinning Ring -->
					<div class="absolute inset-0 border-4 border-primary-500 rounded-full border-t-transparent animate-spin" />

					<!-- Icon Container -->
					<div class="relative bg-white dark:bg-gray-900 rounded-full w-24 h-24 flex items-center justify-center shadow-sm border border-gray-100 dark:border-gray-800">
						<UIcon name="i-heroicons-magnifying-glass" class="w-10 h-10 text-primary-500 animate-pulse" />
					</div>
				</div>

				<div class="text-center space-y-3 max-w-4/5 px-6">
					<div class="text-xl font-medium text-gray-900 dark:text-white">
						正在扫描目录
					</div>
					<div class="text-sm text-gray-500 truncate font-mono bg-gray-50 dark:bg-gray-800 py-2 px-4 rounded-lg border border-gray-100 dark:border-gray-800">
						{{ crawlStats.current_path || '准备中...' }}
					</div>
					<div class="text-sm text-gray-400">
						已发现 <span class="text-gray-900 dark:text-white font-medium">{{ crawlStats.found_files }}</span> 个文件
					</div>
				</div>
			</div>

			<!-- Empty State -->
			<div v-else-if="filteredFiles.length === 0" class="flex-1 flex flex-col items-center justify-center bg-white dark:bg-gray-900">
				<UEmpty
					:icon="files.length === 0 ? 'i-heroicons-document-magnifying-glass' : 'i-heroicons-funnel'"
					:label="files.length === 0 ? '暂无文件' : '无匹配文件'"
					:description="files.length === 0 ? '请点击上方“扫描”按钮开始查找文件' : '当前列表为空'"
				>
					<template #icon>
						<div class="bg-gray-50 dark:bg-gray-800 p-4 rounded-full mb-4">
							<UIcon :name="files.length === 0 ? 'i-heroicons-document-magnifying-glass' : 'i-heroicons-funnel'" class="w-12 h-12 text-gray-400 dark:text-gray-500" />
						</div>
					</template>
				</UEmpty>
			</div>

			<!-- File List -->
			<UScrollArea v-else class="flex-1 bg-white dark:bg-gray-900 p-4">
				<div v-for="file in filteredFiles" :key="file.path" class="group relative mb-3 bg-white dark:bg-gray-800/50 border border-gray-200 dark:border-gray-800 rounded-lg hover:border-primary-500/50 transition-colors shadow-sm last:mb-0 overflow-hidden" @contextmenu.prevent="showContextMenu($event, file)">
					<!-- Background Progress -->
					<div
						class="absolute inset-0 bg-primary-50 dark:bg-primary-900/10 transition-all duration-300 ease-linear pointer-events-none"
						:style="{ width: fileStates[file.path]?.status === 'done' ? '100%' : `${(fileStates[file.path]?.bytesRead ?? 0) / file.size * 100}%` }"
					/>

					<div class="relative p-4 flex items-center gap-3 z-10">
						<!-- Icon -->
						<UAvatar
							:icon="getFileIcon(file.path)"
							size="sm"
							class="bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400 shrink-0"
						/>

						<!-- Content -->
						<div class="flex-1 min-w-0">
							<div class="flex items-center justify-between mb-1.5">
								<div class="flex items-center gap-2 min-w-0">
									<div class="text-sm text-gray-900 dark:text-white truncate font-medium" :title="file.path.split('/').pop()">
										{{ file.path.split('/').pop() }}
									</div>
									<UBadge
										v-if="fileStates[file.path]"
										:color="getStatusColor(fileStates[file.path]?.status)"
										variant="subtle"
										size="xs"
										class="shrink-0"
									>
										{{ getStatusLabel(fileStates[file.path]?.status) }}
									</UBadge>
								</div>

								<div class="flex items-center gap-3 text-xs shrink-0 tabular-nums">
									<template v-if="fileStates[file.path]?.status === 'running'">
										<span class="font-mono text-primary-500 font-medium">
											{{ formatSpeed(fileStates[file.path]?.speed || 0) }}
										</span>
										<span class="text-gray-400">
											{{ formatDuration(fileStates[file.path]?.eta || 0) }}
										</span>
									</template>
								</div>
							</div>

							<div class="flex items-center justify-between gap-4">
								<div class="text-xs text-gray-400 truncate flex-1 font-mono" :title="file.path">
									{{ file.path }}
								</div>

								<div class="flex items-center gap-2 shrink-0">
									<div class="text-[10px] text-gray-500 tabular-nums">
										{{ formatBytes(fileStates[file.path]?.bytesRead || 0) }} / {{ formatBytes(file.size) }}
									</div>
									<UButton
										v-if="fileStates[file.path]?.status === 'failed'"
										icon="i-heroicons-arrow-path"
										size="xs"
										color="primary"
										variant="soft"
										label="重试"
										@click.stop="retryFile(file)"
									/>
								</div>
							</div>

							<div v-if="fileStates[file.path]?.error" class="mt-1.5 text-xs text-red-500 truncate bg-red-50 dark:bg-red-900/10 px-2 py-0.5 rounded inline-block">
								{{ fileStates[file.path]?.error }}
							</div>
						</div>
					</div>
				</div>
			</UScrollArea>

			<!-- Context Menu -->
			<div
				v-if="contextMenu.visible"
				class="fixed z-50 min-w-48"
				:style="{ top: `${contextMenu.y}px`, left: `${contextMenu.x}px` }"
			>
				<div class="fixed inset-0 z-[-1]" @click="closeContextMenu" @contextmenu.prevent="closeContextMenu" />
				<UCard :ui="{ body: 'p-1!' }">
					<div class="flex flex-col gap-0.5">
						<UButton
							variant="ghost"
							color="neutral"
							size="sm"
							icon="i-heroicons-information-circle"
							label="详细信息"
							class="justify-start"
							@click="openDetails"
						/>
						<UButton
							variant="ghost"
							color="neutral"
							size="sm"
							icon="i-heroicons-link"
							label="复制下载链接"
							class="justify-start"
							@click="copyDownloadLink"
						/>
						<UButton
							variant="ghost"
							color="neutral"
							size="sm"
							icon="i-heroicons-folder"
							label="复制文件路径"
							class="justify-start"
							@click="copyFilePath"
						/>
					</div>
				</UCard>
			</div>

			<!-- Details Modal -->
			<UModal
				v-model:open="detailsOpen"
				title="详细信息"
				:description="detailsFile?.path || ''"
				:ui="{ content: 'w-[min(92vw,900px)] max-w-225' }"
			>
				<template #body>
					<div class="space-y-4">
						<div v-if="!detailsFile" class="text-sm text-gray-500">
							未选择文件。
						</div>

						<div v-else class="space-y-4">
							<div class="grid grid-cols-[120px,1fr] gap-x-3 gap-y-2 text-sm">
								<div class="text-gray-500">
									状态
								</div>
								<div class="flex items-center gap-2 min-w-0">
									<UBadge
										:color="getStatusColor(detailsState?.status)"
										variant="subtle"
										size="xs"
									>
										{{ getStatusLabel(detailsState?.status) }}
									</UBadge>
									<span v-if="detailsState?.httpStatus" class="text-xs font-mono text-gray-500">
										HTTP {{ detailsState.httpStatus }}
									</span>
								</div>

								<div class="text-gray-500">
									请求 URL
								</div>
								<div class="font-mono text-xs break-all">
									{{ detailsState?.url || '-' }}
								</div>

								<div class="text-gray-500">
									最终 URL
								</div>
								<div class="font-mono text-xs break-all">
									{{ detailsState?.finalUrl || '-' }}
								</div>

								<div class="text-gray-500">
									读取
								</div>
								<div class="text-xs font-mono tabular-nums">
									{{ formatBytes(detailsState?.bytesRead || 0) }} / {{ formatBytes(detailsFile.size) }}
									<span v-if="detailsState?.contentLength" class="text-gray-400">
										（Content-Length: {{ formatBytes(detailsState.contentLength) }}）
									</span>
								</div>

								<div class="text-gray-500">
									耗时
								</div>
								<div class="text-xs font-mono tabular-nums">
									{{ detailsState?.durationMs ? `${detailsState.durationMs} ms` : '-' }}
								</div>

								<div class="text-gray-500">
									Attempt
								</div>
								<div class="text-xs font-mono tabular-nums">
									{{ detailsState?.attempt ?? '-' }}
								</div>
							</div>

							<div v-if="detailsState?.error" class="text-xs text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/10 px-3 py-2 rounded border border-red-100 dark:border-red-900/30 break-all">
								{{ detailsState.error }}
							</div>

							<div>
								<div class="text-sm font-medium text-gray-900 dark:text-white mb-2">
									响应 Header
								</div>
								<div
									v-if="detailsState?.responseHeaders && detailsState.responseHeaders.length"
									class="max-h-72 overflow-auto rounded border border-gray-200 dark:border-gray-800"
								>
									<div
										v-for="(h, i) in detailsState.responseHeaders"
										:key="i"
										class="grid grid-cols-[220px,1fr] gap-x-3 gap-y-1 px-3 py-2 border-b border-gray-100 dark:border-gray-800 last:border-b-0"
									>
										<div class="text-xs font-mono text-gray-700 dark:text-gray-200 truncate" :title="h.name">
											{{ h.name }}
										</div>
										<div class="text-xs font-mono text-gray-500 break-all" :title="h.value">
											{{ h.value }}
										</div>
									</div>
								</div>
								<div v-else class="text-xs text-gray-500">
									暂无（通常只有文件完成/失败后才会采集 header）。
								</div>
							</div>
						</div>
					</div>
				</template>
			</UModal>
		</UDashboardPanel>
	</UDashboardGroup>
</template>

<script setup lang="ts">
	import type { FileItem, FileUpdate, RunSettings, SiteSettings } from "~/types";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";

	const toast = useToast();

	const siteSettings = ref<SiteSettings | null>(null);
	const files = ref<FileItem[]>([]);
	const crawling = ref(false);
	const running = ref(false);
	const stopping = ref(false);
	const runId = ref<string | null>(null);
	const backendSpeed = ref<number | null>(null);

	const runSettings = ref<RunSettings>({
		minConn: 5,
		maxConn: 32,
		rateLimitBytesPerSec: 10 * 1024 * 1024 // 10MB/s
	});

	const rateLimitMB = computed({
		get: () => runSettings.value.rateLimitBytesPerSec ? runSettings.value.rateLimitBytesPerSec / (1024 * 1024) : 0,
		set: (val) => {
			runSettings.value.rateLimitBytesPerSec = val ? val * 1024 * 1024 : 0;
		}
	});

	// Crawl stats
	const crawlStats = ref({
		scanned_dirs: 0,
		total_dirs: 0,
		found_files: 0,
		current_path: "",
		speed: 0,
		eta: 0
	});
	let crawlLastUpdateTime = 0;
	let crawlLastScannedDirs = 0;
	let crawlSpeedEma = 0;

	// Run stats
	const stats = ref({
		total: 0,
		done: 0,
		failed: 0,
		bytes: 0,
		speed: 0
	});

	interface ExtendedFileState extends FileUpdate {
		speed: number
		eta: number
		lastUpdate: number
	}

	const fileStates = ref<Record<string, ExtendedFileState>>({});
	const selectedTab = ref("all");

	const contextMenu = ref({
		visible: false,
		x: 0,
		y: 0,
		file: null as FileItem | null
	});

	const detailsOpen = ref(false);
	const detailsFile = ref<FileItem | null>(null);
	const detailsState = computed(() => {
		if (!detailsFile.value) return undefined;
		return fileStates.value[detailsFile.value.path];
	});

	function showContextMenu(e: MouseEvent, file: FileItem) {
		contextMenu.value = {
			visible: true,
			x: Math.min(e.clientX, window.innerWidth - 200),
			y: Math.min(e.clientY, window.innerHeight - 100),
			file
		};
	}

	function closeContextMenu() {
		contextMenu.value.visible = false;
	}

	function openDetails() {
		const file = contextMenu.value.file;
		if (!file) return;
		detailsFile.value = file;
		detailsOpen.value = true;
		closeContextMenu();
	}

	async function copyDownloadLink() {
		const file = contextMenu.value.file;
		const settings = siteSettings.value;
		if (!file || !settings) return;

		const base = (settings.downloadBaseUrl || settings.apiBaseUrl).replace(/\/+$/, "");
		const encodedPath = file.path.split("/").map((p) => encodeURIComponent(p)).join("/");
		const url = `${base}/d${encodedPath}`;

		try {
			await navigator.clipboard.writeText(url);
			toast.add({ title: "链接已复制", color: "success" });
		} catch (e) {
			toast.add({ title: "复制失败", description: String(e), color: "error" });
		}
		closeContextMenu();
	}

	async function copyFilePath() {
		const file = contextMenu.value.file;
		if (!file) return;

		try {
			await navigator.clipboard.writeText(file.path);
			toast.add({ title: "路径已复制", color: "success" });
		} catch (e) {
			toast.add({ title: "复制失败", description: String(e), color: "error" });
		}
		closeContextMenu();
	}

	const tabItems = computed(() => [
		{ label: `全部 (${files.value.length})`, value: "all" },
		{ label: `进行中 (${files.value.filter((f) => fileStates.value[f.path]?.status === "running").length})`, value: "running" },
		{ label: `已完成 (${files.value.filter((f) => fileStates.value[f.path]?.status === "done").length})`, value: "done" },
		{ label: `失败 (${files.value.filter((f) => fileStates.value[f.path]?.status === "failed").length})`, value: "failed" }
	]);

	const filteredFiles = computed(() => {
		let list = files.value;
		if (selectedTab.value !== "all") {
			list = files.value.filter((f) => {
				const status = fileStates.value[f.path]?.status;
				if (selectedTab.value === "running") return status === "running";
				if (selectedTab.value === "done") return status === "done";
				if (selectedTab.value === "failed") return status === "failed";
				return false;
			});
		}

		// Sort: Running > Failed > Queued > Done
		return [...list].sort((a, b) => {
			const statusA = fileStates.value[a.path]?.status;
			const statusB = fileStates.value[b.path]?.status;

			const getPriority = (s: string | undefined) => {
				if (s === "running") return 0;
				if (s === "failed") return 1;
				if (s === "done") return 3;
				return 2; // Queued/Undefined
			};

			return getPriority(statusA) - getPriority(statusB);
		});
	});

	async function copyAllLinks() {
		const settings = siteSettings.value;
		if (!settings) return;

		const base = (settings.downloadBaseUrl || settings.apiBaseUrl).replace(/\/+$/, "");
		const links = filteredFiles.value.map((file) => {
			const encodedPath = file.path.split("/").map((p) => encodeURIComponent(p)).join("/");
			return `${base}/d${encodedPath}`;
		});

		if (links.length === 0) {
			toast.add({ title: "列表为空", color: "warning" });
			return;
		}

		try {
			await navigator.clipboard.writeText(links.join("\n"));
			toast.add({ title: `已复制 ${links.length} 个链接`, color: "success" });
		} catch (e) {
			toast.add({ title: "复制失败", description: String(e), color: "error" });
		}
	}

	async function retryAll() {
		const toRetry = files.value.filter((f) => fileStates.value[f.path]?.status === "failed");
		const count = toRetry.length;

		if (count === 0) {
			toast.add({ title: "没有可重试的任务", color: "warning" });
			return;
		}

		try {
			await invoke("retry_files", { files: toRetry });
		} catch (e) {
			toast.add({ title: "重试失败", description: String(e), color: "error" });
			return;
		}

		running.value = true;
		stopping.value = false;

		// Reset UI state for retried files (backend will re-emit updates)
		toRetry.forEach((f) => {
			if (fileStates.value[f.path]) {
				delete fileStates.value[f.path];
			}
		});
		stats.value.failed = Math.max(0, stats.value.failed - count);

		toast.add({ title: `已提交重试 ${count} 个任务`, color: "success" });
	}

	const globalStats = computed(() => {
		const activeFiles = Object.values(fileStates.value).filter((s) => s.status === "running");
		const totalSpeed = activeFiles.reduce((acc, curr) => acc + curr.speed, 0);

		const totalSize = files.value.reduce((acc, curr) => acc + curr.size, 0);

		// Calculate effective read bytes for progress
		// If done or failed, count full size. If running, count bytesRead.
		const effectiveRead = files.value.reduce((acc, f) => {
			const state = fileStates.value[f.path];
			if (!state) return acc;
			if (state.status === "done" || state.status === "failed") {
				return acc + f.size;
			}
			return acc + state.bytesRead;
		}, 0);

		let eta = 0;
		if (totalSpeed > 0) {
			eta = (totalSize - effectiveRead) / totalSpeed;
		}

		return {
			activeCount: activeFiles.length,
			speed: totalSpeed,
			totalRead: Object.values(fileStates.value).reduce((acc, curr) => acc + curr.bytesRead, 0),
			effectiveRead,
			totalSize,
			progress: totalSize > 0 ? (effectiveRead / totalSize) * 100 : 0,
			eta
		};
	});

	onMounted(async () => {
		const saved = localStorage.getItem("site_settings");
		if (!saved) {
			siteSettings.value = {
				id: "",
				name: "Default",
				apiBaseUrl: "",
				token: "",
				startPath: "/",
				proxyUrl: "",
				userAgent: "",
				cookie: "",
				dirPassword: "",
				downloadBaseUrl: "",
				followRedirects: true
			};
		} else {
			siteSettings.value = JSON.parse(saved);
		}

		watch(siteSettings, (newVal) => {
			if (newVal) {
				localStorage.setItem("site_settings", JSON.stringify(newVal));
			}
		}, { deep: true });

		// Listen for updates
		await listen("run:speedUpdate", (event: any) => {
			// Use backend speed for smoother display
			// But we still need to update globalStats.speed for the UI
			// Since globalStats is computed, we might need a separate ref for display speed
			// However, globalStats.speed is derived from fileStates.
			// Let's override it or use a separate variable.
			// Actually, let's just update a ref that globalStats uses if possible,
			// or just let the backend drive the "Total Speed" display.

			// For now, let's store it in a ref and use it in the template if available
			backendSpeed.value = event.payload as number;
		});

		await listen("crawl:progress", (event: any) => {
			const now = Date.now();
			const payload = event.payload || {};
			const scannedDirs = Number(payload.scanned_dirs || 0);
			const totalDirs = Number(payload.total_dirs || 0);
			const remaining = Math.max(0, totalDirs - scannedDirs);

			// Instantaneous speed based on delta progress
			let instSpeed = 0;
			if (crawlLastUpdateTime > 0) {
				const dt = (now - crawlLastUpdateTime) / 1000;
				const dDirs = scannedDirs - crawlLastScannedDirs;
				if (dt > 0.05 && dDirs >= 0) {
					instSpeed = dDirs / dt;
				}
			}

			// EMA smoothing
			// alpha 越大越“跟手”，越小越平滑。
			const alpha = 0.25;
			if (instSpeed > 0) {
				crawlSpeedEma = crawlSpeedEma > 0 ? (crawlSpeedEma * (1 - alpha) + instSpeed * alpha) : instSpeed;
			} else if (crawlSpeedEma > 0) {
				// 没有增量时轻微衰减，避免一直卡在旧速度
				crawlSpeedEma *= 0.98;
			}

			const speed = crawlSpeedEma;
			const eta = speed > 0 ? remaining / speed : 0;

			crawlLastUpdateTime = now;
			crawlLastScannedDirs = scannedDirs;

			crawlStats.value = {
				...payload,
				speed,
				eta
			};
		});

		// Listen for cookie updates from crawler
		await listen("site:cookie_updated", (event: any) => {
			const settings = siteSettings.value;
			if (settings) {
				settings.cookie = event.payload.cookie;
				localStorage.setItem("site_settings", JSON.stringify(settings));
				toast.add({ title: "Cookie 已更新", color: "info" });
			}
		});

		await listen("run:finished", () => {
			running.value = false;
			stopping.value = false;
			toast.add({ title: "任务已结束", color: "info" });
		});

		await listen<FileUpdate>("run:fileUpdate", (event) => {
			const update = event.payload;
			const now = Date.now();

			const oldState = fileStates.value[update.path];
			let speed = 0;
			let eta = 0;

			if (oldState && update.status === "running") {
				const timeDiff = (now - oldState.lastUpdate) / 1000;
				const bytesDiff = update.bytesRead - oldState.bytesRead;

				if (timeDiff > 0) {
					const instantaneousSpeed = bytesDiff / timeDiff;
					// Exponential Moving Average (EMA) for smoothing
					// New Speed = (Previous Speed * (1 - alpha)) + (Instantaneous Speed * alpha)
					// Alpha = 0.1 means we trust the new value 10%, and keep 90% of history.
					// Higher alpha = more responsive, lower alpha = smoother.
					const alpha = 0.2;
					speed = (oldState.speed * (1 - alpha)) + (instantaneousSpeed * alpha);
				} else {
					speed = oldState.speed;
				}

				// Find file size
				const file = files.value.find((f) => f.path === update.path);
				if (file && speed > 0) {
					eta = (file.size - update.bytesRead) / speed;
				}
			} else if (update.status === "running") {
				// First update for this file
				speed = 0;
			}

			fileStates.value[update.path] = {
				...(oldState || {}),
				...update,
				speed: update.status === "running" ? speed : 0,
				eta: update.status === "running" ? eta : 0,
				lastUpdate: now
			} as ExtendedFileState;

			if (update.status === "done") {
				// Fix progress calculation: update file size to actual bytes read
				const file = files.value.find((f) => f.path === update.path);
				if (file) {
					file.size = update.bytesRead;
				}

				stats.value.done++;
				stats.value.bytes += update.bytesRead;
			} else if (update.status === "failed") {
				stats.value.failed++;
			}
		});
	});

	const canStart = computed(() => {
		return files.value.length > 0 && !crawling.value && (stats.value.done + stats.value.failed < stats.value.total);
	});

	function handleAction() {
		if (canStart.value) {
			startRun();
		} else {
			startCrawl();
		}
	}

	async function startCrawl() {
		const settings = siteSettings.value;
		if (!settings) return;

		crawling.value = true;
		crawlLastUpdateTime = 0;
		crawlLastScannedDirs = 0;
		crawlSpeedEma = 0;
		files.value = [];
		stats.value = { total: 0, done: 0, failed: 0, bytes: 0, speed: 0 };
		try {
			const result = await invoke<FileItem[]>("crawl", { settings });
			files.value = result;
			stats.value.total = result.length;
			toast.add({ title: `发现 ${result.length} 个文件`, color: "success" });
		} catch (e) {
			toast.add({ title: "扫描失败", description: String(e), color: "error" });
		} finally {
			crawling.value = false;
		}
	}

	async function startRun() {
		if (!siteSettings.value || files.value.length === 0) return;

		// Filter files that are not done
		const filesToRun = files.value.filter((f) => {
			const state = fileStates.value[f.path];
			return !state || state.status !== "done";
		});

		if (filesToRun.length === 0) {
			toast.add({ title: "所有任务已完成", color: "success" });
			return;
		}

		running.value = true;

		// Calculate initial stats based on what we are keeping
		const alreadyDone = files.value.length - filesToRun.length;

		// We want to reset failed count because we are retrying them
		stats.value = {
			total: files.value.length,
			done: alreadyDone,
			failed: 0,
			bytes: stats.value.bytes, // Keep bytes
			speed: 0
		};

		// Remove old state for re-run files
		filesToRun.forEach((f) => {
			if (fileStates.value[f.path]) {
				delete fileStates.value[f.path];
			}
		});

		try {
			const id = await invoke<string>("start_run", {
				siteSettings: siteSettings.value,
				runSettings: runSettings.value,
				files: filesToRun
			});
			runId.value = id;
			toast.add({ title: "预热已开始", color: "success" });
		} catch (e) {
			toast.add({ title: "启动失败", description: String(e), color: "error" });
			running.value = false;
		}
	}

	async function retryFile(file: FileItem) {
		if (fileStates.value[file.path]?.status !== "failed") return;

		try {
			await invoke("retry_files", { files: [file] });
		} catch (e) {
			toast.add({ title: "重试失败", description: String(e), color: "error" });
			return;
		}

		running.value = true;
		stopping.value = false;

		delete fileStates.value[file.path];
		stats.value.failed = Math.max(0, stats.value.failed - 1);
		toast.add({ title: "已提交重试", color: "info" });
	}

	async function cancelRun() {
		stopping.value = true;
		try {
			await invoke("cancel_run");
			toast.add({ title: "正在停止..." });
			// Do NOT set running = false here. Wait for run:finished event.
		} catch (e) {
			console.error(e);
			stopping.value = false; // Only reset if error
		}
	}

	function formatBytes(bytes: number) {
		if (bytes === 0) return "0 B";
		const k = 1024;
		const sizes = ["B", "KB", "MB", "GB", "TB"];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return `${Number.parseFloat((bytes / k ** i).toFixed(2))} ${sizes[i]}`;
	}

	function formatSpeed(bytesPerSec: number) {
		return `${formatBytes(bytesPerSec)}/s`;
	}

	function formatDuration(seconds: number) {
		if (!Number.isFinite(seconds) || seconds < 0) return "-";
		if (seconds < 60) return `${Math.ceil(seconds)}s`;
		const mins = Math.floor(seconds / 60);
		const secs = Math.ceil(seconds % 60);
		return `${mins}m ${secs}s`;
	}

	function getFileIcon(path: string) {
		const ext = path.split(".").pop()?.toLowerCase();
		if (["jpg", "jpeg", "png", "gif", "webp"].includes(ext || "")) return "i-heroicons-photo";
		if (["mp4", "webm", "mov"].includes(ext || "")) return "i-heroicons-film";
		if (["mp3", "wav"].includes(ext || "")) return "i-heroicons-musical-note";
		if (["pdf", "doc", "docx"].includes(ext || "")) return "i-heroicons-document-text";
		if (["zip", "rar", "7z"].includes(ext || "")) return "i-heroicons-archive-box";
		return "i-heroicons-document";
	}

	function getStatusColor(status: string | undefined) {
		switch (status) {
		case "done": return "success";
		case "failed": return "error";
		case "running": return "primary";
		default: return "neutral";
		}
	}

	function getStatusLabel(status: string | undefined) {
		switch (status) {
		case "done": return "已完成";
		case "failed": return "失败";
		case "running": return "进行中";
		default: return "等待中";
		}
	}
</script>
