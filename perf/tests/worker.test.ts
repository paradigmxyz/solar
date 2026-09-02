import { describe, expect, it } from 'vite-plus/test'

import app from '../src/worker'

describe('performance Worker', () => {
  it('reports its published-data source', async () => {
    const response = await app.request('http://perf.test/api/health', undefined, {
      GITHUB_REPOSITORY: 'example/solar',
      PERF_DATA_REF: 'runs',
    })

    expect(response.status).toBe(200)
    await expect(response.json()).resolves.toEqual({
      source: 'github',
      repository: 'example/solar',
      ref: 'runs',
    })
  })

  it('does not proxy arbitrary paths', async () => {
    const response = await app.request('http://perf.test/api/data/../secrets')

    expect(response.status).toBe(404)
    expect(response.headers.get('access-control-allow-origin')).toBe('*')
  })
})
