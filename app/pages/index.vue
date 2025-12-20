<template>
	<div class="flex items-center justify-center h-screen">
		<UIcon name="i-mingcute-loading-3-fill" class="animate-spin text-4xl text-gray-400" />
	</div>
</template>

<script setup lang="ts">
	const router = useRouter();

	onMounted(() => {
		const settings = localStorage.getItem("site_settings");
		if (settings) {
			router.replace("/preheat");
		} else {
			router.replace("/site");
		}
	});
</script>
