export interface SiteSettings {
	id: string
	name: string
	apiBaseUrl: string
	downloadBaseUrl?: string
	startPath: string
	token?: string
	dirPassword?: string
	proxyUrl?: string
	userAgent?: string
	cookie?: string
	timeoutMs?: number
	followRedirects?: boolean
}

export interface RunSettings {
	minConn?: number
	maxConn: number
	rateLimitBytesPerSec?: number
	chunkBytes?: number
	maxAttempts?: number
	backoffBaseMs?: number
	backoffMaxMs?: number
	partialMode?: {
		enabled: boolean
		maxBytes?: number
	}
	warnOnRedirect?: boolean
}

export interface FileItem {
	id: string
	/**
	 * 属于哪个站点/分组（对应 SiteSettings.id）。
	 * 旧版本/遗留数据可能不存在，因此保持可选。
	 */
	siteId?: string
	path: string
	name: string
	size: number
	isDir: boolean
	modified?: string
	retries?: number
}

export interface FileUpdate {
	runId: string
	/**
	 * 属于哪个站点/分组（对应 SiteSettings.id）。
	 * 旧版本/遗留事件可能不存在，因此保持可选。
	 */
	siteId?: string
	path: string
	status: "queued" | "running" | "done" | "failed" | "canceled"
	attempt?: number
	url?: string
	finalUrl?: string
	contentLength?: number
	responseHeaders?: Array<{ name: string, value: string }>
	bytesRead: number
	httpStatus?: number
	durationMs?: number
	warning?: {
		code: string
		message: string
		context?: any
	}
	error?: string
}
