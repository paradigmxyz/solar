import { Hono } from 'hono'
import { cors } from 'hono/cors'

import { isPublishedDataPath, publishedDataUrl } from './server/githubData'

interface Env {
  GITHUB_REPOSITORY?: string
  GITHUB_TOKEN?: string
  PERF_DATA_REF?: string
}

const app = new Hono<{ Bindings: Env }>()

app.use(
  '/api/*',
  cors({
    origin: '*',
    allowMethods: ['GET', 'OPTIONS'],
  }),
)

app.get('/api/health', (context) =>
  context.json({
    source: 'github',
    repository: context.env.GITHUB_REPOSITORY || 'paradigmxyz/solar',
    ref: context.env.PERF_DATA_REF || 'gh-pages',
  }),
)

app.get('/api/data/*', async (context) => {
  const path = context.req.path.slice('/api/data/'.length)
  if (!isPublishedDataPath(path)) return context.json({ error: 'Unknown data file' }, 404)

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
