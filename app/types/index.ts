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
	path: string
	name: string
	size: number
	isDir: boolean
	modified?: string
	retries?: number
}

export interface FileUpdate {
	runId: string
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
