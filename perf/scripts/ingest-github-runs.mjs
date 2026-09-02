import { execFile } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { promisify } from 'node:util'

import { insert, select } from './lib/clickhouse.mjs'

const exec = promisify(execFile)

function args() {
  const values = Object.create(null)
  const start = process.argv[2] === '--' ? 3 : 2
  for (let index = start; index < process.argv.length; index += 2) {
    const option = process.argv[index]
    const value = process.argv[index + 1]
    if (!option?.startsWith('--') || value === undefined)
      throw new Error(`Invalid argument: ${option}`)
    values[option.slice(2)] = value
  }
  return values
}

async function runs(repository, runId, limit) {
  if (runId) {
    const { stdout } = await exec('gh', [
      'run',
      'view',
      runId,
      '--repo',
      repository,
      '--json',
      'databaseId,headSha,headBranch,createdAt,conclusion,name,displayTitle',
    ])
    return [JSON.parse(stdout)]
  }
  const { stdout } = await exec('gh', [
    'run',
    'list',
    '--repo',
    repository,
    '--workflow',
    'Benchmark',
    '--limit',
    limit,
    '--json',
    'databaseId,headSha,headBranch,createdAt,conclusion,name,displayTitle',
  ])
  return JSON.parse(stdout)
}

async function importedRuns() {
  return new Set(
    (await select('SELECT workflow_run_id FROM runs FINAL')).map((run) =>
      Number(run.workflow_run_id),
    ),
  )
}

async function refreshMetadata(run) {
  if (run.conclusion !== 'success' || !/^[0-9a-f]{40}$/.test(run.headSha)) return false
  const [stored] = await select(
    `SELECT workflow_run_id, commit, branch, pr, started_at, workflow_name, source_schema, raw_results
     FROM runs FINAL WHERE workflow_run_id = ${run.databaseId} LIMIT 1`,
  )
  if (!stored) return false
  await insert('runs', [{ ...stored, title: run.displayTitle || null }])
  return true
}

async function ingest(repository, run, refresh, knownRuns) {
  if (run.conclusion !== 'success' || !/^[0-9a-f]{40}$/.test(run.headSha)) return false
  if (!refresh && knownRuns.has(run.databaseId)) return false
  const directory = await mkdtemp(join(tmpdir(), 'solar-perf-'))
  try {
    await exec('gh', [
      'run',
      'download',
      String(run.databaseId),
      '--repo',
      repository,
      '--name',
      'codegen-runtime-results',
      '--dir',
      directory,
    ])
    await exec('node', [
      resolve('scripts/ingest-run.mjs'),
      '--results',
      join(directory, 'results.json'),
      '--artifacts',
      join(directory, 'artifacts'),
      '--commit',
      run.headSha,
      '--workflow-run',
      String(run.databaseId),
      '--workflow',
      run.name || 'Benchmark',
      '--branch',
      run.headBranch || '',
      '--title',
      run.displayTitle || '',
      '--timestamp',
      run.createdAt,
    ])
    knownRuns.add(run.databaseId)
    return true
  } catch (error) {
    console.warn(`Skipped workflow run ${run.databaseId}: ${error.message}`)
    return false
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
}

const options = args()
const repository = options.repo || process.env.GITHUB_REPOSITORY || 'paradigmxyz/solar'
const limit = options.limit || '10000'
const available = await runs(repository, options['workflow-run'], limit)
const concurrency = Number(options.concurrency || '8')
if (!Number.isSafeInteger(concurrency) || concurrency < 1 || concurrency > 16)
  throw new Error('Concurrency must be an integer from 1 to 16')
const knownRuns = await importedRuns()
console.log(`Scanning ${available.length} GitHub Actions runs with ${concurrency} workers`)
let count = 0
let next = 0
async function worker() {
  while (next < available.length) {
    const run = available[next++]
    count += Number(
      options['metadata-only'] === 'true'
        ? await refreshMetadata(run)
        : await ingest(repository, run, options.refresh === 'true', knownRuns),
    )
  }
}
await Promise.all(Array.from({ length: Math.min(concurrency, available.length) }, worker))
console.log(
  `${options['metadata-only'] === 'true' ? 'Updated' : 'Ingested'} ${count} GitHub Actions run${count === 1 ? '' : 's'}`,
)
