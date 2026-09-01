import { MultiFileDiff } from '@pierre/diffs/react'
import { useEffect, useState } from 'react'
import { loadArtifact } from './data'

interface Props {
  before: { commit: string; benchmark: string; compiler: string }
  after: { commit: string; benchmark: string; compiler: string }
  path: string
  language: string
}

export default function ArtifactDiff({ before, after, path, language }: Props) {
  const [contents, setContents] = useState<[string, string] | null>(null)
  const [style, setStyle] = useState<'split' | 'unified'>('split')
  useEffect(() => {
    Promise.all([
      loadArtifact(before.commit, before.benchmark, before.compiler, path),
      loadArtifact(after.commit, after.benchmark, after.compiler, path),
    ]).then(setContents)
  }, [after.benchmark, after.commit, after.compiler, before.benchmark, before.commit, before.compiler, path])
  if (!contents) return <p className="empty">Loading diff…</p>
  return <div className="artifact-diff">
    <div className="diff-tools"><button className={style === 'split' ? 'active' : ''} onClick={() => setStyle('split')}>Split</button><button className={style === 'unified' ? 'active' : ''} onClick={() => setStyle('unified')}>Unified</button></div>
    <MultiFileDiff oldFile={{ name: path, contents: contents[0], lang: language }} newFile={{ name: path, contents: contents[1], lang: language }} options={{ diffStyle: style, overflow: 'scroll' }} />
  </div>
}
