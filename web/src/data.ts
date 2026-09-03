import { loadGitHubArtifact, loadGitHubRun } from './githubActions'
import type { RunDocument, RunIndex } from './types'

export function loadIndex(): Promise<RunIndex> {
  return Promise.reject(new Error('History needs the performance service'))
}

export function loadRun(commit: string): Promise<RunDocument> {
  return loadGitHubRun(commit)
}

export function loadArtifact(
  commit: string,
  benchmark: string,
  compiler: string,
  storagePath: string,
): Promise<string | null> {
  return loadGitHubArtifact(commit, benchmark, compiler, storagePath)
}
