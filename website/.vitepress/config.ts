import { withMermaid } from 'vitepress-plugin-mermaid'
import { katex } from '@mdit/plugin-katex'
import { defineConfig } from 'vitepress'
import { resolve } from 'path'

export default withMermaid(
  defineConfig({
    title: 'Qianqian Observatory',
    description:
      'Qianqian Engineering Observatory — project control surface, architecture registry, experiment archive',

    srcDir: resolve(__dirname, '..'),
    outDir: resolve(__dirname, '../dist'),

    head: [
      ['link', { rel: 'stylesheet', href: '/katex/katex.min.css' }],
    ],

    markdown: {
      config: (md) => {
        md.use(katex, {
          delimiters: 'all',
          mathFence: true,
        })
      },
    },

    mermaid: {
      theme: 'default',
    },
    mermaidPlugin: {
      class: 'mermaid-diagram',
    },

    themeConfig: {
      nav: [
        { text: 'Control', link: '/control/' },
        { text: 'Architecture', link: '/architecture/' },
        { text: 'Experiments', link: '/experiments/' },
        { text: 'Research', link: '/research/' },
      ],

      sidebar: {
        '/control/': [
          {
            text: 'Control',
            items: [
              { text: 'Project Control', link: '/control/' },
              { text: 'Roadmap', link: '/control/roadmap' },
              { text: 'History', link: '/control/history' },
            ],
          },
        ],
        '/architecture/': [
          {
            text: 'Architecture',
            items: [
              { text: 'Overview', link: '/architecture/' },
              { text: 'Base Kernel K0', link: '/architecture/base-kernel' },
              { text: 'Playback Kernel', link: '/architecture/playback-kernel' },
              { text: 'Plugin Graph', link: '/architecture/plugin-graph' },
              {
                text: 'Control vs Data Plane',
                link: '/architecture/realtime-data-plane',
              },
            ],
          },
        ],
        '/experiments/': [
          {
            text: 'Experiments',
            items: [
              { text: 'Overview', link: '/experiments/' },
              {
                text: 'FFmpeg Minimization',
                link: '/experiments/ffmpeg-minimization',
              },
              {
                text: 'Composition Kernel Oracles',
                link: '/experiments/composition-kernel-oracles',
              },
            ],
          },
        ],
        '/research/': [
          {
            text: 'Research',
            items: [
              { text: 'Overview', link: '/research/' },
              {
                text: 'Spatiotemporal Composability',
                link: '/research/spatiotemporal-composability',
              },
              { text: 'FFmpeg Closure', link: '/research/ffmpeg-closure' },
              { text: 'Realtime Audio', link: '/research/realtime-audio' },
            ],
          },
        ],
      },

      socialLinks: [
        { icon: 'github', link: 'https://github.com/jnhu76/qianqian' },
      ],
    },
  })
)
