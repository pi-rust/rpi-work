import { createElement, useMemo, type ReactNode } from 'react'
import { CopyTextButton } from '@/components/copy-text-button'
import { markdownView, type MarkdownNode } from '@/lib/viewmodel'
import './rich-answer.css'

function nodes(items: MarkdownNode[]): ReactNode[] {
  // Structural positions are presentation keys within the canonical message;
  // there is no independent message/run identity or Markdown parser in React.
  return items.map((node, index) => <Content key={index} node={node} />)
}

function Content({ node }: { node: MarkdownNode }) {
  switch (node.kind) {
    case 'text': return node.text
    case 'image': return <span className="wb-image-placeholder">图片（未加载）：{node.alt}</span>
    case 'task': return <input className="wb-markdown-task" type="checkbox" disabled checked={node.checked} readOnly aria-label={node.checked ? '已勾选的清单项（只读）' : '未勾选的清单项（只读）'} />
    case 'link': return node.href
      ? <a href={node.href} target="_blank" rel="noopener noreferrer" referrerPolicy="no-referrer">{nodes(node.children)}<span className="sr-only">（新窗口）</span></a>
      : <span className="wb-inert-link" title="未启用的链接">{nodes(node.children)}</span>
    case 'codeBlock': return <section className="wb-message-code" aria-label={`${node.language} 代码块`}>
      <div className="wb-message-code-header"><span>{node.language}</span><CopyTextButton text={node.code} label="复制代码" /></div>
      <pre tabIndex={0} role="region" aria-label={`${node.language} 代码，可横向滚动`}><code>{node.tokens.map((token, index) => <span key={index} className={token.class || undefined}>{token.text}</span>)}</code></pre>
    </section>
    case 'element': {
      const children = nodes(node.children)
      if (node.tag === 'br' || node.tag === 'hr') return createElement(node.tag)
      if (node.tag === 'table') return <div className="wb-answer-table" tabIndex={0} role="region" aria-label="回答表格，可横向滚动"><table>{children}</table></div>
      return createElement(node.tag, node.tag === 'ol' && node.start !== undefined ? { start: node.start } : null, children)
    }
  }
}

/** Rust owns the bounded AST, safe links, readable copy payload and lexer.
 * React renders text nodes/allowlisted tags, never dangerous inner HTML. */
export function RichAnswer({ text, action }: { text: string; action?: ReactNode }) {
  const view = useMemo(() => markdownView(text), [text])
  return <>
    <div className="wb-prose" data-plain={view.plain || undefined}>{nodes(view.nodes)}</div>
    {view.plain ? <p role="status" className="text-muted-foreground text-[12px]">内容较复杂，已按原文显示。</p> : null}
    <div className="wb-answer-actions"><CopyTextButton text={view.copyText} label="复制回答" />{action}</div>
  </>
}
