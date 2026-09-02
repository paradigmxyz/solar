import { describe, expect, it } from 'vite-plus/test'

import app from '../src/worker'

describe('website Worker', () => {
  it('reports its published-data source', async () => {
    const response = await app.request('http://web.test/api/health', undefined, {
      GITHUB_REPOSITORY: 'example/solar',
      WEB_DATA_REF: 'runs',
    })

    expect(response.status).toBe(200)
    await expect(response.json()).resolves.toEqual({
      source: 'github',
      repository: 'example/solar',
      ref: 'runs',
    })
  })

  it('reports ClickHouse when configured', async () => {
    const response = await app.request('http://web.test/api/health', undefined, {
      CLICKHOUSE_HOST: 'clickhouse.example',
    })

    await expect(response.json()).resolves.toMatchObject({ source: 'clickhouse' })
  })

  it('does not proxy arbitrary paths', async () => {
    const response = await app.request('http://web.test/api/data/../secrets')

    expect(response.status).toBe(404)
    expect(response.headers.get('access-control-allow-origin')).toBe('*')
  })
})
