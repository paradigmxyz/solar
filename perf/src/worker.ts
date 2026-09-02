import { Hono } from 'hono'
import { cors } from 'hono/cors'

import { isPublishedDataPath, publishedDataUrl } from './server/githubData'

interface Env {
  CLICKHOUSE_DATABASE?: string
  CLICKHOUSE_PASSWORD?: string
  CLICKHOUSE_HOST?: string
  CLICKHOUSE_USER?: string
  GITHUB_REPOSITORY?: string
  GITHUB_TOKEN?: string
  PERF_DATA_REF?: string
}

const app = new Hono<{ Bindings: Env }>()

const commit = /^[0-9a-f]{40}$/
const component = /^[\w.-]+$/

function clickhouseConfigured(env: Env) {
  return Boolean(env.CLICKHOUSE_HOST)
}

async function clickhouse(env: Env, query: string) {
  const host = env.CLICKHOUSE_HOST!
  const url = new URL(
    host.startsWith('http://') || host.startsWith('https://') ? host : `https://${host}`,
  )
  url.searchParams.set('database', env.CLICKHOUSE_DATABASE || 'solar_perf')
  const credentials = btoa(`${env.CLICKHOUSE_USER || 'default'}:${env.CLICKHOUSE_PASSWORD || ''}`)
  const response = await fetch(url, {
    method: 'POST',
    headers: { authorization: `Basic ${credentials}` },
    body: `${query}\nFORMAT JSONEachRow`,
  })
  if (!response.ok) throw new Error(`ClickHouse request failed (${response.status})`)
  const text = await response.text()
  return text
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line) as Record<string, unknown>)
}

function metrics(row: Record<string, unknown>) {
  return {
    compileTime: row.compile_time,
    creationSize: row.bytecode_size,
    runtimeSize: row.runtime_size,
    deployGas: row.deploy_gas,
    runtimeGas: row.total_gas,
    peakMemory: row.peak_rss_bytes,
  }
}

async function indexFromClickHouse(env: Env) {
  const rows = await clickhouse(
    env,
    `SELECT
       r.commit,
       toString(r.started_at) AS timestamp,
       r.branch,
       r.pr,
       countDistinct(b.test_id) AS benchmarkCount,
       sumIf(b.compile_time_seconds, b.compiler = 'solar' AND b.status = 'ok') AS compile_time,
       sumIf(b.bytecode_size, b.compiler = 'solar' AND b.status = 'ok') AS bytecode_size,
       sumIf(b.runtime_size, b.compiler = 'solar' AND b.status = 'ok') AS runtime_size,
       sumIf(b.deploy_gas, b.compiler = 'solar' AND b.status = 'ok') AS deploy_gas,
       sumIf(b.total_gas, b.compiler = 'solar' AND b.status = 'ok') AS total_gas,
       maxIf(b.peak_rss_bytes, b.compiler = 'solar' AND b.status = 'ok') AS peak_rss_bytes
     FROM runs AS r FINAL
     LEFT JOIN benchmark_results AS b FINAL USING workflow_run_id
     GROUP BY r.workflow_run_id, r.commit, r.started_at, r.branch, r.pr
     ORDER BY r.started_at DESC`,
  )
  return {
    schemaVersion: 1,
    updatedAt: new Date().toISOString(),
    runs: rows.map((row) => ({
      commit: row.commit,
      timestamp: row.timestamp,
      branch: row.branch,
      pr: row.pr,
      benchmarkCount: row.benchmarkCount,
      metrics: metrics(row),
    })),
  }
}

async function runFromClickHouse(env: Env, sha: string) {
  const [run] = await clickhouse(
    env,
    `SELECT workflow_run_id, commit, branch, pr, toString(started_at) AS timestamp
     FROM runs FINAL WHERE commit = '${sha}' ORDER BY imported_at DESC LIMIT 1`,
  )
  if (!run) return null
  const runId = Number(run.workflow_run_id)
  const rows = await clickhouse(
    env,
    `SELECT test_id, description, suite, compiler, status, compile_time_seconds, bytecode_size,
       runtime_size, deploy_gas, total_gas, peak_rss_bytes
     FROM benchmark_results FINAL WHERE workflow_run_id = ${runId}
     ORDER BY test_id, compiler`,
  )
  const results = new Map<string, Record<string, unknown>>()
  for (const row of rows) {
    const testId = String(row.test_id)
    const result = results.get(testId) ?? {
      test_id: testId,
      description: row.description,
      suite: row.suite,
      compilers: {},
    }
    ;(result.compilers as Record<string, unknown>)[String(row.compiler)] = {
      status: row.status,
      compile_time_seconds: row.compile_time_seconds,
      bytecode_size: row.bytecode_size,
      runtime_size: row.runtime_size,
      deploy_gas: row.deploy_gas,
      total_gas: row.total_gas,
      peak_rss_bytes: row.peak_rss_bytes,
    }
    results.set(testId, result)
  }
  const artifactRows = await clickhouse(
    env,
    `SELECT test_id, path, storage_path, label, language, max(length(content)) AS bytes,
       groupArray(compiler) AS compilers
     FROM artifact_files FINAL WHERE workflow_run_id = ${runId}
     GROUP BY test_id, path, storage_path, label, language
     ORDER BY test_id, storage_path`,
  )
  const artifacts: Record<string, unknown[]> = {}
  for (const row of artifactRows) {
    const testId = String(row.test_id)
    ;(artifacts[testId] ??= []).push({
      path: row.path,
      storagePath: row.storage_path,
      label: row.label,
      language: row.language,
      bytes: row.bytes,
      compilers: row.compilers,
    })
  }
  return { schemaVersion: 1, ...run, results: [...results.values()], artifacts }
}

app.use(
  '/api/*',
  cors({
    origin: '*',
    allowMethods: ['GET', 'OPTIONS'],
  }),
)

app.get('/api/health', (context) =>
  context.json({
    source: clickhouseConfigured(context.env) ? 'clickhouse' : 'github',
    repository: context.env.GITHUB_REPOSITORY || 'paradigmxyz/solar',
    ref: context.env.PERF_DATA_REF || 'gh-pages',
  }),
)

app.get('/api/data/*', async (context) => {
  const path = context.req.path.slice('/api/data/'.length)
  if (!isPublishedDataPath(path)) return context.json({ error: 'Unknown data file' }, 404)

  if (clickhouseConfigured(context.env)) {
    try {
      if (path === 'index.json') return context.json(await indexFromClickHouse(context.env))
      const [sha, benchmark, compiler, storagePath] = path.replace(/^runs\//, '').split('/')
      if (!commit.test(sha)) return context.json({ error: 'Unknown data file' }, 404)
      if (path.endsWith('/run.json')) {
        const run = await runFromClickHouse(context.env, sha)
        return run ? context.json(run) : context.json({ error: 'Run not found' }, 404)
      }
      if (
        !component.test(benchmark) ||
        !component.test(compiler) ||
        !/^\d+\.json$/.test(storagePath)
      )
        return context.json({ error: 'Unknown data file' }, 404)
      const [run] = await clickhouse(
        context.env,
        `SELECT workflow_run_id FROM runs FINAL WHERE commit = '${sha}' ORDER BY imported_at DESC LIMIT 1`,
      )
      if (!run) return context.json({ error: 'Run not found' }, 404)
      const [artifact] = await clickhouse(
        context.env,
        `SELECT content FROM artifact_files FINAL
         WHERE workflow_run_id = ${Number(run.workflow_run_id)}
           AND test_id = '${benchmark}' AND compiler = '${compiler}' AND storage_path = '${storagePath}'
         ORDER BY imported_at DESC LIMIT 1`,
      )
      return artifact
        ? new Response(JSON.stringify(artifact.content), {
            headers: { 'content-type': 'application/json' },
          })
        : context.json({ error: 'Artifact not found' }, 404)
    } catch (error) {
      console.error(error)
      return context.json({ error: 'Performance data is unavailable' }, 503)
    }
  }

  const repository = context.env.GITHUB_REPOSITORY || 'paradigmxyz/solar'
  const ref = context.env.PERF_DATA_REF || 'gh-pages'
  const headers = new Headers({ accept: 'application/json' })
  if (context.env.GITHUB_TOKEN) headers.set('authorization', `Bearer ${context.env.GITHUB_TOKEN}`)

  const response = await fetch(publishedDataUrl(repository, ref, path), { headers })
  if (!response.ok)
    return Response.json({ error: 'Published data is unavailable' }, { status: response.status })

  return new Response(response.body, {
    headers: {
      'cache-control': path === 'index.json' ? 'public, max-age=60' : 'public, max-age=3600',
      'content-type': 'application/json; charset=utf-8',
    },
  })
})

app.notFound((context) => context.json({ error: 'Not found' }, 404))

export default app
