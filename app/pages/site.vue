<template>
	<div class="p-4 max-w-4xl mx-auto">
		<h1 class="text-2xl font-bold mb-6">
			站点配置
		</h1>

		<UForm :schema="schema" :state="state" class="space-y-6" @submit="onSubmit">
			<UCard>
				<template #header>
					<h2 class="text-lg font-semibold">
						基本信息
					</h2>
				</template>

				<div class="grid gap-4">
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

			<UCard>
				<template #header>
					<h2 class="text-lg font-semibold">
						预热目标
					</h2>
				</template>

				<div class="grid gap-4">
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

			<UCard>
				<template #header>
					<h2 class="text-lg font-semibold">
						网络设置
					</h2>
				</template>

				<div class="grid gap-4">
					<UFormField label="代理 URL" name="proxyUrl" help="http://127.0.0.1:7890">
						<UInput v-model="state.proxyUrl" />
					</UFormField>

					<UFormField label="User Agent" name="userAgent">
						<UInput v-model="state.userAgent" />
					</UFormField>

					<div class="flex gap-4">
						<UFormField label="超时时间 (ms)" name="timeoutMs" class="flex-1">
							<UInput v-model="state.timeoutMs" type="number" />
						</UFormField>

						<UFormField label="跟随重定向" name="followRedirects" class="flex-1">
							<UCheckbox v-model="state.followRedirects" />
						</UFormField>
					</div>
				</div>
			</UCard>

			<div class="flex justify-end gap-4">
				<UButton type="submit" color="primary">
					保存并继续
				</UButton>
			</div>
		</UForm>
	</div>
</template>

<script setup lang="ts">
	import type { FormSubmitEvent } from "#ui/types";
	import type { SiteSettings } from "~/types";
	import { listen } from "@tauri-apps/api/event";
	import { z } from "zod";

	const state = ref<SiteSettings>({
		id: "",
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
	});

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

	// Load from local storage on mount (mock persistence for M1)
	onMounted(async () => {
		const saved = localStorage.getItem("site_settings");
		if (saved) {
			try {
				const parsed = JSON.parse(saved);
				// Migration: if userAgent is the old default, update it
				if (parsed.userAgent && parsed.userAgent.includes("Chrome/120.0.0.0")) {
					parsed.userAgent = "olist-cdn-preheat/0.1 (Mozilla/5.0 compatible)";
				}
				state.value = { ...state.value, ...parsed };
			} catch (e) {
				console.error("Failed to load settings", e);
			}
		}

		// Listen for cookie updates from backend (WAF solver)
		await listen<{ cookie: string, userAgent?: string }>("site:cookie_updated", (event) => {
			console.log("Received new cookie from backend");
			state.value.cookie = event.payload.cookie;
			if (event.payload.userAgent) {
				state.value.userAgent = event.payload.userAgent;
			}
			// Auto-save
			localStorage.setItem("site_settings", JSON.stringify(state.value));
			toast.add({ title: "已自动更新安全凭证", color: "success" });
		});
	});

	async function onSubmit(_event: FormSubmitEvent<Schema>) {
		// Save to local storage
		localStorage.setItem("site_settings", JSON.stringify(state.value));

		toast.add({ title: "设置已保存", color: "success" });

		// Navigate to preheat
		router.push("/preheat");
	}

	function onApiUrlChange() {
		if (!state.value.downloadBaseUrl && state.value.apiBaseUrl) {
			state.value.downloadBaseUrl = state.value.apiBaseUrl;
		}
	}
</script>
