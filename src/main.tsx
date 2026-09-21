import { toast } from 'sonner'
window.addEventListener('unhandledrejection', (e) => {
  const msg = e.reason instanceof Error ? e.reason.message : String(e.reason)
  toast.error(msg)
})
import React from 'react'
import ReactDOM from 'react-dom/client'
import { getCurrentWindow } from '@tauri-apps/api/window'
import App from './App'
import Overlay from './overlay'
import Notch from './notch'
import './styles/globals.css'

// Follow the Mac's appearance; every token has a dark value.
const mq = window.matchMedia('(prefers-color-scheme: dark)')
const applyTheme = () => document.documentElement.classList.toggle('dark', mq.matches)
applyTheme()
mq.addEventListener('change', applyTheme)

const label = getCurrentWindow().label
const isOverlay = label === 'overlay'
const isNotch = label === 'notch'
if (isOverlay) document.documentElement.classList.add('overlay')
if (isNotch) document.documentElement.classList.add('notch')

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>{isNotch ? <Notch /> : isOverlay ? <Overlay /> : <App />}</React.StrictMode>,
)
