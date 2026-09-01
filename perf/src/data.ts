import type { RunDocument, RunIndex } from './types'

const localRoot = `${import.meta.env.BASE_URL}data/`
const githubRoot = 'https://raw.githubusercontent.com/paradigmxyz/solar/gh-pages/data/'
const configuredRoot = import.meta.env.VITE_PERF_DATA_URL
const normalizeRoot = (root: string) => root.endsWith('/') ? root : `${root}/`
let activeRoot = normalizeRoot(configuredRoot || (import.meta.env.DEV ? githubRoot : localRoot))
let rootResolved = Boolean(configuredRoot) || !import.meta.env.DEV
let indexPromise: Promise<RunIndex> | null = null

async function getJson<T>(root: string, path: string, fresh = false): Promise<T> {
  const response = await fetch(`${root}${path}`, fresh ? { cache: 'no-store' } : undefined)
  if (!response.ok) throw new Error(`Could not load ${root}${path}`)
  return response.json() as Promise<T>
}

export function loadIndex() {
  if (!indexPromise) {
    indexPromise = (async () => {
      const roots = (configuredRoot || !import.meta.env.DEV) ? [activeRoot] : [githubRoot, localRoot]
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

export async function loadArtifact(commit: string, benchmark: string, compiler: string, storagePath: string) {
  const parts = [commit, benchmark, compiler, ...storagePath.split('/')].map(encodeURIComponent)
  const response = await fetch(`${await dataRoot()}runs/${parts.join('/')}`)
  if (!response.ok) return ''
  return response.json() as Promise<string>
}
