import { strFromU8, unzipSync } from 'fflate'
import type { ArtifactFile, BenchmarkResult, RunDocument } from './types'

const repository = 'paradigmxyz/solar'
const api = `https://api.github.com/repos/${repository}`
const tokenKey = 'solar-web-github-token'
const artifactCache = 'solar-web-actions-artifacts-v1'

const artifactFiles = new Map([
  ['input.json', ['Compiler input', 'json', '0.json']],
  ['output.json', ['Compiler output', 'json', '1.json']],
  ['mir.mir', ['MIR', 'text', '2.json']],
  ['creation.evmir', ['Creation EVM IR', 'text', '3.json']],
  ['runtime.evmir', ['Runtime EVM IR', 'text', '4.json']],
  ['optimized-ir.yul', ['Optimized Yul IR', 'solidity', '5.json']],
  ['creation.disasm', ['Creation disassembly', 'asm', '6.json']],
  ['runtime.disasm', ['Runtime disassembly', 'asm', '7.json']],
  ['creation.hex', ['Creation bytecode', 'text', '8.json']],
  ['runtime.hex', ['Runtime bytecode', 'text', '9.json']],
])

interface WorkflowRun {
  id: number
  name: string
  display_title: string
  head_branch: string | null
  head_sha: string
  created_at: string
  pull_requests: { number: number }[]
}

interface GitHubArtifact {
  id: number
  name: string
  expired: boolean
  archive_download_url: string
}

interface LoadedRun {
  document: RunDocument
  files: Map<string, Uint8Array>
}

export class GitHubTokenRequired extends Error {
  constructor() {
    super('GitHub access is required to download Actions artifacts')
  }
}

const runs = new Map<string, Promise<LoadedRun>>()

export function hasGitHubToken() {
  return Boolean(localStorage.getItem(tokenKey))
}

export function setGitHubToken(token: string) {
  const value = token.trim()
  if (value) localStorage.setItem(tokenKey, value)
  else localStorage.removeItem(tokenKey)
  runs.clear()
}

async function request<T>(url: string, authenticated = false): Promise<T> {
  const headers = new Headers({ accept: 'application/vnd.github+json' })
  const token = localStorage.getItem(tokenKey)
  if (authenticated && !token) throw new GitHubTokenRequired()
  if (token) headers.set('authorization', `Bearer ${token}`)
  const response = await fetch(url, { headers })
  if (response.status === 401 && authenticated) throw new GitHubTokenRequired()
  if (!response.ok) throw new Error(`GitHub request failed (${response.status})`)
  return response.json() as Promise<T>
}

async function resolveCommit(commit: string) {
  if (commit.length === 40) return commit
  const data = await request<{ sha: string }>(`${api}/commits/${encodeURIComponent(commit)}`)
  return data.sha
}

async function workflowRun(commit: string) {
  const data = await request<{ workflow_runs: WorkflowRun[] }>(
    `${api}/actions/runs?head_sha=${encodeURIComponent(commit)}&per_page=100`,
  )
  for (const run of data.workflow_runs) {
    const artifacts = await request<{ artifacts: GitHubArtifact[] }>(
      `${api}/actions/runs/${run.id}/artifacts`,
    )
    const artifact = artifacts.artifacts.find(
      (candidate) => candidate.name === 'codegen-runtime-results' && !candidate.expired,
    )
    if (artifact) return { run, artifact }
  }
  throw new Error('No retained codegen benchmark artifact exists for this commit')
}

async function artifactBytes(artifact: GitHubArtifact) {
  const key = new Request(`${location.origin}/solar-actions-artifact/${artifact.id}.zip`)
  if ('caches' in window) {
    try {
      const cache = await caches.open(artifactCache)
      const cached = await cache.match(key)
      if (cached) return new Uint8Array(await cached.arrayBuffer())

      const response = await fetch(artifact.archive_download_url, {
        headers: { authorization: `Bearer ${localStorage.getItem(tokenKey)!}` },
      })
      if (response.status === 401) throw new GitHubTokenRequired()
      if (!response.ok) throw new Error(`Could not download artifact (${response.status})`)
      try {
        await cache.put(key, response.clone())
      } catch {}
      return new Uint8Array(await response.arrayBuffer())
    } catch (error) {
      if (error instanceof GitHubTokenRequired) throw error
    }
  }

  const response = await fetch(artifact.archive_download_url, {
    headers: { authorization: `Bearer ${localStorage.getItem(tokenKey)!}` },
  })
  if (response.status === 401) throw new GitHubTokenRequired()
  if (!response.ok) throw new Error(`Could not download artifact (${response.status})`)
  return new Uint8Array(await response.arrayBuffer())
}

function archivePath(files: Map<string, Uint8Array>, path: string) {
  return [...files.keys()].find((name) => name === path || name.endsWith(`/${path}`))
}

function artifactManifest(files: Map<string, Uint8Array>, results: BenchmarkResult[]) {
  const artifacts: Record<string, ArtifactFile[]> = {}
  for (const result of results) {
    const entries = new Map<string, ArtifactFile>()
    for (const compiler of ['solar', 'solc']) {
      const prefix = `artifacts/${result.test_id}/${compiler}/`
      for (const [path, content] of files) {
        const index = path.lastIndexOf(prefix)
        if (index < 0) continue
        const name = path.slice(index + prefix.length)
        const metadata = artifactFiles.get(name)
        if (!metadata) continue
        const entry = entries.get(name) ?? {
          path: name,
          storagePath: metadata[2],
          label: metadata[0],
          language: metadata[1],
          bytes: content.byteLength,
          compilers: [],
        }
        entry.bytes = Math.max(entry.bytes, content.byteLength)
        entry.compilers.push(compiler)
        entries.set(name, entry)
      }
    }
    if (entries.size) artifacts[result.test_id] = [...entries.values()]
  }
  return artifacts
}

async function load(commit: string): Promise<LoadedRun> {
  const sha = await resolveCommit(commit)
  const { run, artifact } = await workflowRun(sha)
  const files = new Map(Object.entries(unzipSync(await artifactBytes(artifact))))
  const resultsPath = archivePath(files, 'results.json')
  if (!resultsPath) throw new Error('Benchmark artifact has no results file')
  const parsed = JSON.parse(strFromU8(files.get(resultsPath)!)) as
    | { results?: BenchmarkResult[] }
    | BenchmarkResult[]
  const results = Array.isArray(parsed) ? parsed : parsed.results
  if (!Array.isArray(results)) throw new Error('Benchmark artifact has invalid results')
  return {
    document: {
      schemaVersion: 1,
      commit: sha,
      branch: run.head_branch,
      pr: run.pull_requests[0]?.number ?? null,
      title: run.display_title,
      timestamp: run.created_at,
      results,
      artifacts: artifactManifest(files, results),
    },
    files,
  }
}

export async function loadGitHubRun(commit: string) {
  const key = commit.toLowerCase()
  const cached = runs.get(key) ?? load(key)
  runs.set(key, cached)
  try {
    return (await cached).document
  } catch (error) {
    runs.delete(key)
    throw error
  }
}

export async function loadGitHubArtifact(
  commit: string,
  benchmark: string,
  compiler: string,
  storagePath: string,
) {
  const key = commit.toLowerCase()
  const cached = runs.get(key) ?? load(key)
  runs.set(key, cached)
  try {
    const { document, files } = await cached
    const artifact = document.artifacts[benchmark]?.find((file) => file.storagePath === storagePath)
    if (!artifact || !artifact.compilers.includes(compiler)) return null
    const path = archivePath(files, `artifacts/${benchmark}/${compiler}/${artifact.path}`)
    return path ? strFromU8(files.get(path)!) : null
  } catch (error) {
    runs.delete(key)
    throw error
  }
}
