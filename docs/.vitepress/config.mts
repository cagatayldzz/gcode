import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'gCode',
  description: 'User and developer documentation for the gCode macOS app.',
  lang: 'en',
  base: `${(process.env.DOCS_BASE ?? '/').replace(/\/$/, '')}/`,
  cleanUrls: true,
  // Not a dot-directory: the Pages artifact upload skips hidden folders and would publish nothing.
  outDir: '../docs-site',
  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/', activeMatch: '^/guide/' },
      { text: 'Architecture', link: '/architecture/', activeMatch: '^/architecture/' },
      { text: 'Development', link: '/development/', activeMatch: '^/development/' },
      { text: 'Releases', link: 'https://github.com/cagatayldzz/vekro/releases' }
    ],
    sidebar: {
      '/guide/': [
        {
          text: 'Start',
          items: [{ text: 'Getting started', link: '/guide/' }]
        },
        {
          text: 'Workspace',
          items: [
            { text: 'Projects', link: '/guide/projects' },
            { text: 'Explorer & editor', link: '/guide/editor' },
            { text: 'Search & navigation', link: '/guide/search' }
          ]
        },
        {
          text: 'Terminals',
          items: [
            { text: 'Terminals', link: '/guide/terminals' },
            { text: 'AI agents', link: '/guide/agents' }
          ]
        },
        {
          text: 'Git',
          items: [
            { text: 'Changes & commits', link: '/guide/git-changes' },
            { text: 'History', link: '/guide/git-history' },
            { text: 'Branches, sync & merging', link: '/guide/git-branches' }
          ]
        },
        {
          text: 'Configure',
          items: [
            { text: 'Accounts', link: '/guide/accounts' },
            { text: 'Settings', link: '/guide/settings' },
            { text: 'Keyboard shortcuts', link: '/guide/shortcuts' }
          ]
        }
      ],
      '/architecture/': [
        {
          text: 'Architecture',
          items: [
            { text: 'Overview', link: '/architecture/' },
            { text: 'Code map', link: '/architecture/code-map' },
            { text: 'Window layout', link: '/architecture/window-layout' },
            { text: 'Workspace & state', link: '/architecture/workspace' }
          ]
        },
        {
          text: 'Subsystems',
          items: [
            { text: 'Terminals', link: '/architecture/terminal' },
            { text: 'Git', link: '/architecture/git' },
            { text: 'Editor & performance', link: '/architecture/editor' },
            { text: 'Hosting & accounts', link: '/architecture/hosting' },
            { text: 'Commit messages', link: '/architecture/commit-messages' },
            { text: 'Updates', link: '/architecture/updates' },
            { text: 'Extensions', link: '/architecture/extensions' },
            { text: 'Persistence', link: '/architecture/persistence' }
          ]
        }
      ],
      '/development/': [
        {
          text: 'Development',
          items: [
            { text: 'Setup & commands', link: '/development/' },
            { text: 'Conventions', link: '/development/conventions' },
            { text: 'Testing', link: '/development/testing' },
            { text: 'Performance', link: '/development/performance' },
            { text: 'Releases & CI', link: '/development/releases' }
          ]
        }
      ]
    },
    outline: { level: [2, 3] },
    search: { provider: 'local' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/cagatayldzz/vekro' }]
  }
})
