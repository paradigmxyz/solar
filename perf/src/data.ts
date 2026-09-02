import type { RunDocument, RunIndex } from './types'

const localRoot = `${import.meta.env.BASE_URL}data/`
const githubRoot = 'https://raw.githubusercontent.com/paradigmxyz/solar/gh-pages/data/'
const configuredApi =
  import.meta.env.VITE_PERF_API_URL ||
  (['127.0.0.1', 'localhost'].includes(window.location.hostname)
    ? 'http://127.0.0.1:8788'
    : undefined)
const configuredDataRoot = import.meta.env.VITE_PERF_DATA_URL
const configuredRoot = configuredApi
  ? `${configuredApi.replace(/\/$/, '')}/api/data/`
  : configuredDataRoot
const normalizeRoot = (root: string) => (root.endsWith('/') ? root : `${root}/`)
let activeRoot = normalizeRoot(configuredRoot || (import.meta.env.DEV ? githubRoot : localRoot))
let rootResolved = Boolean(configuredDataRoot) || !import.meta.env.DEV
let indexPromise: Promise<RunIndex> | null = null

function fallbackRoots(root: string) {
  if (configuredDataRoot) return [root]
  if (configuredApi) return [...new Set([root, localRoot, githubRoot])]
  return import.meta.env.DEV ? [localRoot, githubRoot] : [root]
}

async function getJson<T>(root: string, path: string, fresh = false): Promise<T> {
  const response = await fetch(`${root}${path}`, fresh ? { cache: 'no-store' } : undefined)
  if (!response.ok) throw new Error(`Could not load ${root}${path}`)
  return response.json() as Promise<T>
}

export function loadIndex() {
  if (!indexPromise) {
    indexPromise = (async () => {
      const roots = fallbackRoots(activeRoot)
      let failure: unknown
      for (const root of roots) {
        try {
          const normalized = normalizeRoot(root)
          const index = await getJson<RunIndex>(normalized, 'index.json', true)
          activeRoot = normalized
          rootResolved = true
          return index
        } catch (error) {
          failure = error
        }
      }
      throw failure
    })().catch((error) => {
      indexPromise = null
      throw error
    })
  }
  return indexPromise
}

async function dataRoot() {
  if (!rootResolved) {
    await loadIndex()
  }
  return activeRoot
}

export async function loadRun(commit: string) {
  return getJson<RunDocument>(await dataRoot(), `runs/${encodeURIComponent(commit)}/run.json`)
}

export async function loadArtifact(
  commit: string,
  benchmark: string,
  compiler: string,
  storagePath: string,
): Promise<string | null> {
  const parts = [commit, benchmark, compiler, ...storagePath.split('/')].map(encodeURIComponent)
  const root = await dataRoot()
  const roots = fallbackRoots(root)
  for (const candidate of roots) {
    const response = await fetch(`${candidate}runs/${parts.join('/')}`)
    if (response.status === 404) continue
    if (!response.ok) throw new Error(`Could not load artifact: ${response.statusText}`)
    const contents = await response.text()
    if (/^\s*<!doctype html/i.test(contents)) continue
    try {
      return JSON.parse(contents) as string
    } catch {
      throw new Error('Could not parse artifact JSON')
    }
  }
  return null
}
