import '@fontsource-variable/geist'
import '@fontsource-variable/bricolage-grotesque'
import '@fontsource/pixelify-sans/500.css'
import '@fontsource-variable/jetbrains-mono'
import './app.css'
import { mount } from 'svelte'
import App from './App.svelte'

mount(App, { target: document.getElementById('app')! })
