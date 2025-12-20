<template>
	<div class="grid place-items-center py-12 md:py-24">
		<div class="flex flex-col items-center gap-y-4 md:gap-y-8">
			<p class="text-(--ui-success) font-semibold">
				404
			</p>
			<div class="text-center space-y-3">
				<h1 class="text-3xl font-bold tracking-tight" sm="text-5xl">
					页面未找到
				</h1>
				<p class="text-base text-(--ui-muted) leading-7">
					抱歉，我们找不到您要访问的页面。
				</p>
			</div>
			<UButton to="/" variant="outline" size="lg" :ui="{ base: 'px-5' }">
				返回首页
			</UButton>
		</div>
	</div>
</template>

<script lang="ts" setup>
	definePageMeta({
		layout: "blank"
	});
</script>
