import { katex } from '@mdit/plugin-katex'
import { defineConfig } from 'vitepress'
import { resolve } from 'path'

export default defineConfig({
  title: 'Qianqian 工程观测站',
  description:
    'Qianqian 工程观测站 —— 项目控制台、架构登记处与实验档案',

  srcDir: resolve(__dirname, '..'),
  outDir: resolve(__dirname, '../dist'),

  vite: {
    server: {
      host: '0.0.0.0',
      // OneDrive holds 5173 (Bound, non-listening) on this machine, so
      // dev always falls back to 5174; pin the port to stay deterministic.
      port: 5174,
      hmr: {
        host: 'localhost',
        protocol: 'ws',
      },
    },
    optimizeDeps: {
      // Pre-bundle Mermaid once in dev so its transitive CJS internals get
      // proper CJS->ESM interop. We name the public package boundary only —
      // never Mermaid's internals (enforced by scripts/verify.mjs).
      include: ['mermaid'],
    },
  },

  head: [
    ['link', { rel: 'stylesheet', href: '/katex/katex.min.css' }],

    // 霞鹜文楷(LXGW WenKai)webfont —— 引入方式 A(本站采用):
    // jsDelivr 切片版 style.css。字体按 unicode-range 切成约百片 woff2,
    // 浏览器只下载页面实际用到的切片,避免整包中文字体过大。
    // preconnect 提前建立到 CDN 的连接,降低首字渲染延迟。
    ['link', { rel: 'preconnect', href: 'https://cdn.jsdelivr.net' }],
    [
      'link',
      {
        rel: 'stylesheet',
        href: 'https://cdn.jsdelivr.net/npm/lxgw-wenkai-webfont@1.7.0/style.css',
      },
    ],
  ],

  markdown: {
    config: (md) => {
      md.use(katex, {
        delimiters: 'all',
        mathFence: true,
      })

      // Fenced ```mermaid blocks render through the Qianqian-owned
      // MermaidDiagram seam; every other fence keeps the default behavior.
      const defaultFence = md.renderer.rules.fence.bind(md.renderer.rules)
      md.renderer.rules.fence = (tokens, idx, options, env, self) => {
        const token = tokens[idx]
        if (token.info.trim() === 'mermaid') {
          return `<MermaidDiagram code="${encodeURIComponent(token.content)}" />`
        }
        return defaultFence(tokens, idx, options, env, self)
      }
    },
  },

  themeConfig: {
    nav: [
      { text: '项目控制', link: '/control/' },
      { text: '架构', link: '/architecture/' },
      { text: '实验', link: '/experiments/' },
      { text: '研究', link: '/research/' },
    ],

    sidebar: {
      '/control/': [
        {
          text: '项目控制',
          items: [
            { text: '项目控制', link: '/control/' },
            { text: '路线图', link: '/control/roadmap' },
            { text: '历史', link: '/control/history' },
          ],
        },
      ],
      '/architecture/': [
        {
          text: '架构',
          items: [
            { text: '总览', link: '/architecture/' },
            { text: 'Base Kernel K0', link: '/architecture/base-kernel' },
            { text: 'Playback Architecture', link: '/architecture/playback-kernel' },
            { text: '插件图', link: '/architecture/plugin-graph' },
            {
              text: '控制平面与数据平面',
              link: '/architecture/realtime-data-plane',
            },
          ],
        },
      ],
      '/experiments/': [
        {
          text: '实验',
          items: [
            { text: '总览', link: '/experiments/' },
            {
              text: 'FFmpeg 最小化',
              link: '/experiments/ffmpeg-minimization',
            },
            {
              text: 'Composition Kernel Oracle 测试',
              link: '/experiments/composition-kernel-oracles',
            },
          ],
        },
      ],
      '/research/': [
        {
          text: '研究',
          items: [
            { text: '总览', link: '/research/' },
            {
              text: '时空可组合性',
              link: '/research/spatiotemporal-composability',
            },
            { text: 'FFmpeg 闭包', link: '/research/ffmpeg-closure' },
            { text: '实时音频', link: '/research/realtime-audio' },
            {
              text: 'DSH → Composition Kernel 演进',
              link: '/research/dsh-composition-lineage',
            },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/jnhu76/qianqian' },
    ],
  },
})
