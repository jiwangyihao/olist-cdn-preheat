<template>
	<div class="p-3 sm:p-4 max-w-6xl mx-auto">
		<div class="flex items-start justify-between gap-3 mb-4">
			<div>
				<h1 class="text-xl sm:text-2xl font-bold">
					站点配置
				</h1>
				<p class="text-sm text-gray-500 mt-1">
					支持添加多个站点；预热/扫描将使用当前选中的站点。
				</p>
			</div>
			<div class="flex gap-2 shrink-0">
				<UButton icon="i-heroicons-plus" color="primary" variant="soft" size="sm" label="新增站点" @click="addSite" />
				<UButton
					icon="i-heroicons-trash"
					color="error"
					variant="ghost"
					size="sm"
					label="删除"
					:disabled="sites.length <= 1"
					@click="requestRemoveActiveSite"
				/>
			</div>
		</div>

		<div class="grid grid-cols-1 lg:grid-cols-[280px,1fr] gap-4">
			<!-- Site list -->
			<UCard :ui="{ body: 'p-2' }">
				<template #header>
					<div class="flex items-center justify-between">
						<h2 class="text-sm font-semibold">
							站点列表
						</h2>
						<span class="text-xs text-gray-500 tabular-nums">{{ sites.length }}</span>
					</div>
				</template>

				<div class="flex flex-col gap-1">
					<button
						v-for="s in sites"
						:key="s.id"
						type="button"
						class="w-full text-left rounded-md px-2 py-2 border transition-colors"
						:class="s.id === activeSiteId ? 'border-primary-500/60 bg-primary-50 dark:bg-primary-900/10' : 'border-transparent hover:bg-gray-50 dark:hover:bg-gray-800/40'"
						@click="selectSite(s.id)"
					>
						<div class="flex items-center justify-between gap-2">
							<div class="min-w-0">
								<div class="text-sm font-medium truncate">
									{{ s.name || '未命名站点' }}
								</div>
								<div class="text-xs text-gray-500 truncate font-mono" :title="s.apiBaseUrl">
									{{ s.apiBaseUrl || '（未填写 API URL）' }}
								</div>
							</div>
							<UBadge v-if="s.id === activeSiteId" color="primary" size="xs" variant="subtle">
								当前
							</UBadge>
						</div>
					</button>
				</div>
			</UCard>

			<!-- Editor -->
			<UForm :schema="schema" :state="state" class="space-y-4" @submit="onSubmit">
				<UCard :ui="{ body: 'p-4' }">
					<template #header>
						<div class="flex items-center justify-between gap-3">
							<h2 class="text-base font-semibold">
								快速配置（推荐）
							</h2>
							<UButton
								:label="showAdvanced ? '隐藏高级设置' : '显示高级设置'"
								variant="ghost"
								color="neutral"
								size="xs"
								@click.prevent="showAdvanced = !showAdvanced"
							/>
						</div>
					</template>

					<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
						<UFormField label="站点名称" name="name" help="可留空，粘贴/填写 URL 时会自动推断">
							<UInput v-model="state.name" placeholder="例如：demo-olist /photos" />
						</UFormField>

						<UFormField label="API 基础 URL" name="apiBaseUrl" help="例如：https://alist.example.com">
							<UInput v-model="state.apiBaseUrl" placeholder="https://..." @blur="onApiUrlChange" />
						</UFormField>

						<UFormField label="起始路径" name="startPath" help="逻辑路径，例如：/videos">
							<UInput v-model="state.startPath" placeholder="/" />
						</UFormField>
					</div>

					<div class="mt-3 grid grid-cols-1 gap-2">
						<UFormField
							label="粘贴站点链接（可多行）"
						>
							<template #help>
								<div class="whitespace-pre-line">
									支持：
									1) https://demo-olist.example.com/photos
									2) 名称 https://demo-olist.example.com/photos（名称可省略）
									3) 名称 | https://demo-olist.example.com | /photos（或不写名称：https://demo-olist.example.com | /photos）
									多行会自动批量新增/更新站点
								</div>
							</template>
							<UTextarea v-model="quickPaste" :rows="3" :placeholder="quickPastePlaceholder" />
						</UFormField>
						<div class="flex flex-wrap gap-2 justify-end">
							<UButton
								icon="i-heroicons-clipboard"
								color="neutral"
								variant="soft"
								size="sm"
								label="从剪贴板粘贴"
								@click.prevent="pasteFromClipboard"
							/>
							<UButton
								icon="i-heroicons-arrow-up-on-square"
								color="neutral"
								variant="soft"
								size="sm"
								label="导出当前站点"
								@click.prevent="exportActiveSite"
							/>
							<UButton
								icon="i-heroicons-arrow-up-on-square-stack"
								color="neutral"
								variant="soft"
								size="sm"
								label="导出所有站点"
								:disabled="sites.length === 0"
								@click.prevent="exportAllSites"
							/>
							<UButton
								icon="i-heroicons-bolt"
								color="primary"
								variant="soft"
								size="sm"
								label="解析并应用"
								@click.prevent="applyQuickPaste"
							/>
						</div>
					</div>

					<div class="mt-3 flex items-center justify-between gap-3">
						<p class="text-xs text-gray-500">
							大多数情况下只需要这两项即可开始扫描/预热。
						</p>
						<div class="flex gap-2">
							<UButton type="submit" color="primary" size="sm">
								保存并进入预热
							</UButton>
						</div>
					</div>
				</UCard>

				<div v-show="showAdvanced" class="space-y-4">
					<UCard :ui="{ body: 'p-4' }">
						<template #header>
							<div class="flex items-center justify-between gap-3">
								<h2 class="text-base font-semibold">
									基本信息
								</h2>
								<div class="text-xs text-gray-500 font-mono truncate max-w-[45%]" :title="state.id">
									{{ state.id }}
								</div>
							</div>
						</template>

						<div class="grid gap-3">
							<UFormField label="站点名称" name="name">
								<UInput v-model="state.name" />
							</UFormField>

							<UFormField label="API 基础 URL" name="apiBaseUrl" help="例如：https://alist.example.com">
								<UInput v-model="state.apiBaseUrl" @blur="onApiUrlChange" />
							</UFormField>

							<UFormField label="Token" name="token" help="公开站点可留空">
								<UInput v-model="state.token" type="password" />
							</UFormField>
						</div>
					</UCard>

					<UCard :ui="{ body: 'p-4' }">
						<template #header>
							<h2 class="text-base font-semibold">
								预热目标
							</h2>
						</template>

						<div class="grid gap-3">
							<UFormField label="起始路径" name="startPath" help="逻辑路径，例如：/videos">
								<UInput v-model="state.startPath" />
							</UFormField>

							<UFormField label="目录密码" name="dirPassword">
								<UInput v-model="state.dirPassword" type="password" />
							</UFormField>

							<UFormField label="下载基础 URL" name="downloadBaseUrl" help="CDN 域名，例如：https://cdn.example.com">
								<UInput v-model="state.downloadBaseUrl" placeholder="默认与 API 基础 URL 相同" />
							</UFormField>
						</div>
					</UCard>

					<UCard :ui="{ body: 'p-4' }">
						<template #header>
							<h2 class="text-base font-semibold">
								网络设置
							</h2>
						</template>

						<div class="grid gap-3">
							<UFormField label="代理 URL" name="proxyUrl" help="http://127.0.0.1:7890">
								<UInput v-model="state.proxyUrl" />
							</UFormField>

							<UFormField label="User Agent" name="userAgent">
								<UInput v-model="state.userAgent" />
							</UFormField>

							<div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
								<UFormField label="超时时间 (ms)" name="timeoutMs">
									<UInput v-model="state.timeoutMs" type="number" />
								</UFormField>

								<UFormField label="跟随重定向" name="followRedirects">
									<UCheckbox v-model="state.followRedirects" />
								</UFormField>
							</div>
						</div>
					</UCard>
				</div>
			</UForm>
		</div>

		<UModal v-model:open="deleteConfirmOpen" title="删除站点" :description="deleteConfirmDescription">
			<template #body>
				<div class="text-sm text-gray-600 dark:text-gray-300">
					将永久删除该站点配置（不可撤销）。
				</div>
			</template>
			<template #footer>
				<div class="flex justify-end gap-2">
					<UButton color="neutral" variant="ghost" @click="deleteConfirmOpen = false">
						取消
					</UButton>
					<UButton color="error" @click="confirmRemoveActiveSite">
						删除
					</UButton>
				</div>
			</template>
		</UModal>
	</div>
</template>

<script setup lang="ts">
	import type { FormSubmitEvent } from "#ui/types";
	import type { SiteSettings } from "~/types";
	import { listen } from "@tauri-apps/api/event";
	import { computed } from "vue";
	import { z } from "zod";

	const STORAGE_SINGLE_KEY = "site_settings";
	const STORAGE_LIST_KEY = "site_settings_list";
	const STORAGE_ACTIVE_KEY = "site_settings_active_id";

	function newSiteId() {
		try {
			return crypto.randomUUID();
		} catch {
			return `${Date.now()}_${Math.random().toString(16).slice(2)}`;
		}
	}

	function createDefaultSite(): SiteSettings {
		return {
			id: newSiteId(),
			name: "我的站点",
			apiBaseUrl: "",
			downloadBaseUrl: "",
			startPath: "/",
			token: "",
			dirPassword: "",
			proxyUrl: "",
			userAgent: "olist-cdn-preheat/0.1 (Mozilla/5.0 compatible)",
			cookie: "",
			timeoutMs: 60000,
			followRedirects: true
		};
	}

	const sites = ref<SiteSettings[]>([]);
	const activeSiteId = ref<string>("");
	const showAdvanced = ref(false);
	const quickPaste = ref("");
	const quickPastePlaceholder = "每行一条：\n1) https://demo-olist.example.com/photos\n2) 名称 https://demo-olist.example.com/photos\n3) 名称 | https://demo-olist.example.com | /photos\n\n多行会自动批量新增站点";
	const isBulkApplying = ref(false);
	const deleteConfirmOpen = ref(false);
	const deleteConfirmDescription = computed(() => {
		const current = sites.value.find((s) => s.id === activeSiteId.value);
		return current?.name ? `确定删除站点「${current.name}」吗？` : "确定删除当前站点吗？";
	});

	const state = ref<SiteSettings>(createDefaultSite());

	const schema = z.object({
		name: z.string().min(1, "名称不能为空"),
		apiBaseUrl: z.string().url("无效的 URL"),
		startPath: z.string().startsWith("/", "必须以 / 开头"),
		downloadBaseUrl: z.string().url("无效的 URL").optional().or(z.literal("")),
		token: z.string().optional(),
		dirPassword: z.string().optional(),
		proxyUrl: z.string().optional(),
		userAgent: z.string().optional(),
		cookie: z.string().optional(),
		timeoutMs: z.number().min(1000),
		followRedirects: z.boolean()
	});

	type Schema = z.output<typeof schema>;

	const toast = useToast();
	const router = useRouter();

	function persistAll() {
		localStorage.setItem(STORAGE_LIST_KEY, JSON.stringify(sites.value));
		localStorage.setItem(STORAGE_ACTIVE_KEY, activeSiteId.value);
		// 兼容旧逻辑：仍然写入单站点 key，供 index/preheat 等旧读取路径使用。
		localStorage.setItem(STORAGE_SINGLE_KEY, JSON.stringify(state.value));
	}

	function loadSitesFromStorage() {
		const listRaw = localStorage.getItem(STORAGE_LIST_KEY);
		const activeRaw = localStorage.getItem(STORAGE_ACTIVE_KEY);
		if (listRaw) {
			try {
				const parsed = JSON.parse(listRaw) as SiteSettings[];
				const normalized = (parsed || []).map((s) => ({
					...createDefaultSite(),
					...s,
					id: s?.id || newSiteId()
				}));
				sites.value = normalized.length > 0 ? normalized : [createDefaultSite()];

				const pickedId = (activeRaw && sites.value.some((s) => s.id === activeRaw))
					? activeRaw
					: sites.value[0]?.id;
				activeSiteId.value = pickedId || sites.value[0]!.id;

				const active = sites.value.find((s) => s.id === activeSiteId.value) || sites.value[0];
				if (!active) {
					const fresh = createDefaultSite();
					sites.value = [fresh];
					activeSiteId.value = fresh.id;
					state.value = { ...fresh };
					persistAll();
					return;
				}
				state.value = { ...active };
				return;
			} catch (e) {
				console.error("Failed to load site list", e);
			}
		}

		// Migration: old single-site storage
		const savedSingle = localStorage.getItem(STORAGE_SINGLE_KEY);
		if (savedSingle) {
			try {
				const parsed = JSON.parse(savedSingle) as Partial<SiteSettings>;
				const migrated: SiteSettings = {
					...createDefaultSite(),
					...parsed,
					id: (parsed.id && String(parsed.id).length > 0) ? String(parsed.id) : newSiteId()
				};

				// Migration: if userAgent is the old default, update it
				if (migrated.userAgent && migrated.userAgent.includes("Chrome/120.0.0.0")) {
					migrated.userAgent = "olist-cdn-preheat/0.1 (Mozilla/5.0 compatible)";
				}

				sites.value = [migrated];
				activeSiteId.value = migrated.id;
				state.value = { ...migrated };
				persistAll();
				return;
			} catch (e) {
				console.error("Failed to migrate settings", e);
			}
		}

		// Fresh install
		const fresh = createDefaultSite();
		sites.value = [fresh];
		activeSiteId.value = fresh.id;
		state.value = { ...fresh };
		persistAll();
	}

	function selectSite(id: string) {
		const found = sites.value.find((s) => s.id === id);
		if (!found) return;
		activeSiteId.value = id;
		state.value = { ...found };
		persistAll();
	}

	function upsertActiveSite() {
		const idx = sites.value.findIndex((s) => s.id === activeSiteId.value);
		if (idx >= 0) {
			sites.value[idx] = { ...sites.value[idx], ...state.value, id: activeSiteId.value };
		} else {
			sites.value.push({ ...state.value });
			activeSiteId.value = state.value.id;
		}
	}

	function addSite() {
		const s = createDefaultSite();
		s.name = `站点 ${sites.value.length + 1}`;
		sites.value.unshift(s);
		activeSiteId.value = s.id;
		state.value = { ...s };
		persistAll();
	}

	function requestRemoveActiveSite() {
		if (sites.value.length <= 1) return;
		deleteConfirmOpen.value = true;
	}

	function confirmRemoveActiveSite() {
		if (sites.value.length <= 1) return;
		deleteConfirmOpen.value = false;

		const idx = sites.value.findIndex((s) => s.id === activeSiteId.value);
		if (idx < 0) return;
		sites.value.splice(idx, 1);
		if (sites.value.length === 0) {
			const fresh = createDefaultSite();
			sites.value = [fresh];
			activeSiteId.value = fresh.id;
			state.value = { ...fresh };
		} else {
			const first = sites.value[0]!;
			activeSiteId.value = first.id;
			state.value = { ...first };
		}
		persistAll();
		toast.add({ title: "已删除站点", color: "info" });
	}

	// Load from local storage on mount
	onMounted(async () => {
		loadSitesFromStorage();

		// Listen for cookie updates from backend (WAF solver)
		await listen<{ cookie: string, userAgent?: string }>("site:cookie_updated", (event) => {
			console.log("Received new cookie from backend");
			state.value.cookie = event.payload.cookie;
			if (event.payload.userAgent) {
				state.value.userAgent = event.payload.userAgent;
			}
			// Auto-save
			upsertActiveSite();
			persistAll();
			toast.add({ title: "已自动更新安全凭证", color: "success" });
		});
	});

	async function onSubmit(_event: FormSubmitEvent<Schema>) {
		upsertActiveSite();
		persistAll();

		toast.add({ title: "设置已保存", color: "success" });

		// Navigate to preheat
		router.push("/preheat");
	}

	function onApiUrlChange() {
		// 允许用户直接粘贴“带路径”的链接，比如：https://host/photos
		// 自动拆分为 apiBaseUrl=origin, startPath=/photos
		if (state.value.apiBaseUrl) {
			const parsed = parseSiteLink(state.value.apiBaseUrl);
			if (parsed) {
				state.value.apiBaseUrl = parsed.apiBaseUrl;
				if (state.value.startPath === "/" || !state.value.startPath) {
					state.value.startPath = parsed.startPath;
				}
				if (!state.value.downloadBaseUrl) {
					state.value.downloadBaseUrl = parsed.apiBaseUrl;
				}
				if ((!state.value.name || state.value.name === "我的站点") && parsed.name) {
					state.value.name = parsed.name;
				}
			}
		}

		if (!state.value.downloadBaseUrl && state.value.apiBaseUrl) {
			state.value.downloadBaseUrl = state.value.apiBaseUrl;
		}
		// 如果站点名还是默认值（或空），尝试从 URL 推断一个更友好的名字
		if ((!state.value.name || state.value.name === "我的站点") && state.value.apiBaseUrl) {
			const inferred = inferSiteName(state.value.apiBaseUrl);
			if (inferred) state.value.name = inferred;
		}
	}

	function safeDecodePath(pathname: string) {
		// URL.pathname 可能带 %XX，逐段 decode 比整体 decode 更稳
		const parts = pathname.split("/").map((p) => {
			try {
				return decodeURIComponent(p);
			} catch {
				return p;
			}
		});
		const joined = parts.join("/");
		return joined.startsWith("/") ? joined : `/${joined}`;
	}

	function buildNameFromHostPath(host: string, startPath: string) {
		const clean = (startPath || "/").trim();
		if (clean === "/" || clean === "") return host;
		const segs = clean.split("/").filter(Boolean);
		if (segs.length === 0) return host;
		let suffix = `/${segs.join("/")}`;
		if (suffix.length > 24) suffix = `/${segs[0]}`;
		return `${host} ${suffix}`;
	}

	function parseSiteLink(input: string): { apiBaseUrl: string, startPath: string, name: string } | null {
		const line = (input || "").trim();
		if (!line) return null;

		// 支持的输入格式：
		// 1) https://host/path
		// 2) 名称 https://host/path
		// 3) 名称 | https://host | /path
		// 4) https://host | /path
		const hasPipe = line.includes("|") || line.includes("\t");
		if (hasPipe) {
			const parts = line
				.split(/\||\t/)
				.map((p) => p.trim())
				.filter(Boolean);

			let namePart = "";
			let apiPart = "";
			let pathPart = "";

			for (const p of parts) {
				if (!apiPart && /^https?:\/\//i.test(p)) apiPart = p;
				else if (!pathPart && p.startsWith("/")) pathPart = p;
				else if (!namePart) namePart = p;
				else namePart = `${namePart} ${p}`;
			}

			if (apiPart) {
				try {
					const u = new URL(apiPart);
					const apiBaseUrl = u.origin;
					const startPath = pathPart ? safeDecodePath(pathPart) : safeDecodePath(u.pathname || "/");
					const name = (namePart || "").trim() || buildNameFromHostPath(u.host, startPath);
					return { apiBaseUrl, startPath, name };
				} catch {
					return null;
				}
			}
		}

		// 名称 + URL（名称可省略）
		const m = line.match(/^(?:(.+?)\s+)?(https?:\/\/\S+)$/i);
		if (m) {
			const maybeName = (m[1] || "").trim();
			const urlPart = (m[2] || "").trim();
			try {
				const u = new URL(urlPart);
				const apiBaseUrl = u.origin;
				const startPath = safeDecodePath(u.pathname || "/");
				const inferred = buildNameFromHostPath(u.host, startPath);
				const name = maybeName || inferred;
				return { apiBaseUrl, startPath, name };
			} catch {
				return null;
			}
		}

		return null;
	}

	function formatExportLine(s: Pick<SiteSettings, "name" | "apiBaseUrl" | "startPath">) {
		const name = (s.name || "").trim();
		const api = (s.apiBaseUrl || "").trim();
		const start = (s.startPath || "/").trim() || "/";
		if (name) return `${name} | ${api} | ${start}`;
		return `${api} | ${start}`;
	}

	async function writeToClipboard(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			return true;
		} catch {
			return false;
		}
	}

	async function exportActiveSite() {
		upsertActiveSite();
		persistAll();
		const text = formatExportLine(state.value);
		quickPaste.value = text;
		const ok = await writeToClipboard(text);
		toast.add({ title: ok ? "已导出到剪贴板" : "已生成导出内容（复制失败）", color: ok ? "success" : "warning" });
	}

	async function exportAllSites() {
		upsertActiveSite();
		persistAll();
		const text = sites.value
			.map((s) => formatExportLine(s))
			.filter((l) => l.trim().length > 0)
			.join("\n");
		quickPaste.value = text;
		const ok = await writeToClipboard(text);
		toast.add({ title: ok ? "已导出到剪贴板" : "已生成导出内容（复制失败）", color: ok ? "success" : "warning" });
	}

	function parseLines(text: string) {
		const normalized = normalizePastedText(text);
		return normalized
			.split(/\n/)
			.map((s) => s.trim())
			.filter(Boolean);
	}

	function normalizePastedText(text: string) {
		// 兼容两类情况：
		// 1) 真实换行（Windows 常见 \r\n）
		// 2) 某些来源复制出来的“字面量 \n / \r\n”（例如从 JSON/日志中复制）
		return (text || "")
			.replace(/\r\n/g, "\n")
			.replace(/\\r\\n/g, "\n")
			.replace(/\\n/g, "\n")
			.replace(/\r/g, "\n");
	}

	function addOrUpdateSiteByParsed(p: { apiBaseUrl: string, startPath: string, name: string }) {
		const idx = sites.value.findIndex((s) => (s.apiBaseUrl || "").replace(/\/+$/, "") === p.apiBaseUrl && (s.startPath || "/") === p.startPath);
		if (idx >= 0) {
			// 若已有同站点+路径，更新名称（但尊重用户自定义：如果不是默认名则不强改）
			const existing = sites.value[idx]!;
			if (!existing.name || existing.name === "我的站点" || existing.name === existing.apiBaseUrl) {
				sites.value[idx] = { ...existing, name: p.name };
			}
			return sites.value[idx]!;
		}

		const s = createDefaultSite();
		s.apiBaseUrl = p.apiBaseUrl;
		s.downloadBaseUrl = p.apiBaseUrl;
		s.startPath = p.startPath;
		s.name = p.name;
		sites.value.unshift(s);
		return s;
	}

	function applyParsedToActive(p: { apiBaseUrl: string, startPath: string, name: string }) {
		state.value.apiBaseUrl = p.apiBaseUrl;
		state.value.startPath = p.startPath;
		if (!state.value.downloadBaseUrl) state.value.downloadBaseUrl = p.apiBaseUrl;
		if (!state.value.name || state.value.name === "我的站点") state.value.name = p.name;
	}

	async function pasteFromClipboard() {
		try {
			const text = await navigator.clipboard.readText();
			if (!text || !text.trim()) {
				toast.add({ title: "剪贴板为空", color: "warning" });
				return;
			}
			quickPaste.value = normalizePastedText(text);
			await applyQuickPaste();
		} catch (e) {
			toast.add({ title: "读取剪贴板失败", description: String(e), color: "error" });
		}
	}

	async function applyQuickPaste() {
		const lines = parseLines(quickPaste.value);
		if (lines.length === 0) {
			toast.add({ title: "没有可解析的链接", color: "warning" });
			return;
		}

		const parsed = lines
			.map((l) => parseSiteLink(l))
			.filter(Boolean) as Array<{ apiBaseUrl: string, startPath: string, name: string }>;

		if (parsed.length === 0) {
			toast.add({ title: "未识别到有效链接", description: "请粘贴以 http(s):// 开头的链接", color: "warning" });
			return;
		}

		isBulkApplying.value = true;
		try {
			if (parsed.length === 1) {
				const one = parsed[0]!;
				applyParsedToActive(one);
				upsertActiveSite();
				persistAll();
				toast.add({ title: "已应用到当前站点", color: "success" });
				return;
			}

			const created: SiteSettings[] = [];
			for (const p of parsed) {
				created.push(addOrUpdateSiteByParsed(p));
			}

			const first = created[0]!;
			activeSiteId.value = first.id;
			state.value = { ...first };
			persistAll();
			toast.add({ title: `已新增/更新 ${created.length} 个站点`, color: "success" });
		} finally {
			isBulkApplying.value = false;
		}
	}

	function inferSiteName(apiBaseUrl: string) {
		try {
			const u = new URL(apiBaseUrl);
			return u.host;
		} catch {
			return "";
		}
	}

	watch(state, () => {
		if (isBulkApplying.value) return;
		// 轻量的自动保存：编辑时同步写入列表（避免切换站点丢数据）。
		upsertActiveSite();
		persistAll();
	}, { deep: true });
</script>
