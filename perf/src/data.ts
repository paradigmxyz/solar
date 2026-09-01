import type { RunDocument, RunIndex } from './types'

const base = import.meta.env.BASE_URL

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(`${base}data/${path}`)
  if (!response.ok) throw new Error(`Could not load ${path}`)
  return response.json() as Promise<T>
}

export function loadIndex() {
  return getJson<RunIndex>('index.json')
}

export function loadRun(commit: string) {
  return getJson<RunDocument>(`runs/${encodeURIComponent(commit)}/run.json`)
}

export async function loadArtifact(commit: string, benchmark: string, compiler: string, storagePath: string) {
  const parts = [commit, benchmark, compiler, ...storagePath.split('/')].map(encodeURIComponent)
  const response = await fetch(`${base}data/runs/${parts.join('/')}`)
  if (!response.ok) return ''
  return response.json() as Promise<string>
}
