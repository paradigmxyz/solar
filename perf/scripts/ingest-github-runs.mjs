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

async function imported(runId) {
  return (
    (await select(`SELECT 1 FROM runs FINAL WHERE workflow_run_id = ${runId} LIMIT 1`)).length > 0
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

async function ingest(repository, run, refresh) {
  if (run.conclusion !== 'success' || !/^[0-9a-f]{40}$/.test(run.headSha)) return false
  if (!refresh && (await imported(run.databaseId))) return false
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
let count = 0
for (const run of available) {
  count += Number(
    options['metadata-only'] === 'true'
      ? await refreshMetadata(run)
      : await ingest(repository, run, options.refresh === 'true'),
  )
}
console.log(
  `${options['metadata-only'] === 'true' ? 'Updated' : 'Ingested'} ${count} GitHub Actions run${count === 1 ? '' : 's'}`,
)
