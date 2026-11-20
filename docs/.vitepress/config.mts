import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Vekro',
  description: 'User and developer documentation for the Vekro macOS app.',
  lang: 'en',
  base: `${(process.env.VEKRO_DOCS_BASE ?? '/vekro').replace(/\/$/, '')}/`,
  themeConfig: {
    nav: [
      { text: 'Usage', link: '/usage' },
      { text: 'Development', link: '/development' },
      { text: 'Releases', link: 'https://github.com/cagatayldzz/vekro/releases' }
    ],
    sidebar: [
      {
        text: 'User Guide',
        items: [{ text: 'Usage', link: '/usage' }]
      },
      {
        text: 'Developer Guide',
        items: [
          { text: 'Development', link: '/development' },
          { text: 'Architecture', link: '/architecture' }
        ]
      }
    ],
    search: { provider: 'local' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/cagatayldzz/vekro' }]
  }
})
