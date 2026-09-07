import type { Theme } from 'vitepress'
import DefaultTheme from 'vitepress/theme'
import StatusBadge from '../../components/StatusBadge.vue'
import ProvenancePanel from '../../components/ProvenancePanel.vue'
import ClaimBadge from '../../components/ClaimBadge.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('StatusBadge', StatusBadge)
    app.component('ProvenancePanel', ProvenancePanel)
    app.component('ClaimBadge', ClaimBadge)
  },
} satisfies Theme
