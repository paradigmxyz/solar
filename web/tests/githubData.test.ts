import { describe, expect, it } from 'vite-plus/test'

import { isPublishedDataPath, publishedDataUrl } from '../src/server/githubData'

describe('published GitHub data', () => {
  it('accepts the index, run documents, and stored artifacts', () => {
    const commit = '0123456789abcdef0123456789abcdef01234567'
    expect(isPublishedDataPath('index.json')).toBe(true)
    expect(isPublishedDataPath(`runs/${commit}/run.json`)).toBe(true)
    expect(isPublishedDataPath(`runs/${commit}/factorial/solar/7.json`)).toBe(true)
  })

  it('rejects paths outside the published data layout', () => {
    expect(isPublishedDataPath('../index.json')).toBe(false)
    expect(isPublishedDataPath('runs/main/run.json')).toBe(false)
    expect(
      isPublishedDataPath('runs/0123456789abcdef0123456789abcdef01234567/factorial/solar/x.json'),
    ).toBe(false)
  })

  it('uses encoded GitHub raw URLs', () => {
    expect(publishedDataUrl('paradigmxyz/solar', 'gh-pages', 'index.json')).toBe(
      'https://raw.githubusercontent.com/paradigmxyz/solar/gh-pages/data/index.json',
    )
  })
})
