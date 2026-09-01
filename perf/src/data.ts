import type { RunDocument, RunIndex } from './types'

const localRoot = `${import.meta.env.BASE_URL}data/`
const githubRoot = 'https://raw.githubusercontent.com/paradigmxyz/solar/gh-pages/data/'
const configuredRoot = import.meta.env.VITE_PERF_DATA_URL
const normalizeRoot = (root: string) => root.endsWith('/') ? root : `${root}/`
let activeRoot = normalizeRoot(configuredRoot || (import.meta.env.DEV ? githubRoot : localRoot))

async function getJson<T>(root: string, path: string, fresh = false): Promise<T> {
  const response = await fetch(`${root}${path}`, fresh ? { cache: 'no-store' } : undefined)
  if (!response.ok) throw new Error(`Could not load ${root}${path}`)
  return response.json() as Promise<T>
}

export async function loadIndex() {
  const roots = (configuredRoot || !import.meta.env.DEV) ? [activeRoot] : [githubRoot, localRoot]
  let failure: unknown
  for (const root of roots) {
    try {
      const normalized = normalizeRoot(root)
      const index = await getJson<RunIndex>(normalized, 'index.json', true)
      activeRoot = normalized
      return index
    } catch (error) {
      failure = error
    }
  }
  throw failure
}

export function loadRun(commit: string) {
  return getJson<RunDocument>(activeRoot, `runs/${encodeURIComponent(commit)}/run.json`)
}

export async function loadArtifact(commit: string, benchmark: string, compiler: string, storagePath: string) {
  const parts = [commit, benchmark, compiler, ...storagePath.split('/')].map(encodeURIComponent)
  const response = await fetch(`${activeRoot}runs/${parts.join('/')}`)
  if (!response.ok) return ''
  return response.json() as Promise<string>
}
