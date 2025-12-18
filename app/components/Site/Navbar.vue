<template>
	<header class="border-b border-gray-200 dark:border-gray-800 bg-white/75 dark:bg-gray-900/75 backdrop-blur sticky top-0 z-50">
		<div class="mx-auto px-4 sm:px-6 lg:px-8 max-w-7xl flex items-center justify-between h-16">
			<div class="flex items-center gap-4">
				<NuxtLink to="/" class="group/logo flex items-center gap-2">
					<SvgoLogo :font-controlled="false" class="opacity-70 group-hover/logo:opacity-100 transition-opacity size-6" />
					<span class="font-bold">Nuxtor</span>
				</NuxtLink>
			</div>

			<UNavigationMenu
				:items="pages"
				variant="link"
				class="hidden md:flex"
			/>

			<div class="flex items-center gap-2">
				<UBadge variant="subtle">
					Tauri v{{ tauriVersion }}
				</UBadge>
			</div>
		</div>
	</header>
</template>

<script lang="ts" setup>
	const { pages } = usePages();
	const tauriVersion = ref("...");

	onMounted(() => {
		useTauriAppGetTauriVersion()
			.then((v) => {
				tauriVersion.value = v;
			})
			.catch(() => {
				tauriVersion.value = "Unknown";
			});
	});
</script>
