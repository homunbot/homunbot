# Homun UI Redesign — Editorial Canvas

> Reference: HTML mockup della chat passato dall'utente (2026-03-17)
> Status: SPEC — da approvare prima dell'esecuzione

---

## Design Direction

**Da**: Olive Moss Console (toni caldi, muschio, carta invecchiata)
**A**: Editorial Canvas (bianco/nero, accento blu default, editoriale moderno)

**Font**: Plus Jakarta Sans (gia importato)
**Ispirazione**: Notion, Linear, Vercel — minimalismo editoriale con profondita sottile

---

## 1. Palette — Nuovi Token Default

### Light Theme (default)
```
--bg:             #F9F9F8    (quasi-bianco caldo)
--bg-subtle:      #F3F3F2    (sfondo secondario)
--surface:        #FFFFFF    (card/panel)
--surface-raised: #FFFFFF    (overlay)
--surface-hover:  #F5F5F4    (hover state)

--accent:         #3B82F6    (blue-500, default)
--accent-hover:   #2563EB    (blue-600)
--accent-active:  #1D4ED8    (blue-700)
--accent-light:   #EFF6FF    (blue-50)
--accent-border:  #93C5FD    (blue-300)
--accent-text:    #3B82F6    (blue-500)

--t1:             #111111    (primary text — quasi-nero)
--t2:             #6B7280    (secondary — gray-500)
--t3:             #9CA3AF    (tertiary — gray-400)
--t4:             #D1D5DB    (quaternary — gray-300)

--border:         #E5E7EB    (gray-200)
--border-subtle:  #F3F4F6    (gray-100)
--border-strong:  #D1D5DB    (gray-300)

--ok:             #10B981    (emerald-500)
--warn:           #F59E0B    (amber-500)
--err:            #EF4444    (red-500)
```

### Dark Theme
```
--bg:             #0A0A0A    (quasi-nero)
--bg-subtle:      #141414
--surface:        #1A1A1A
--surface-raised: #222222
--surface-hover:  #2A2A2A

--t1:             #F9F9F8
--t2:             #A3A3A3
--t3:             #737373
--t4:             #525252

--border:         #2A2A2A
--border-subtle:  #1F1F1F
--border-strong:  #3D3D3D
```

### Nav Bar
```
--nav-bg:         #0A0A0A    (default: nero)
--nav-text:       rgba(255,255,255,0.4)
--nav-text-hover: #FFFFFF
--nav-text-active: #FFFFFF
--nav-active-bg:  #FFFFFF    (bottone attivo: sfondo bianco, testo nero)
--nav-icon-size:  20px
--nav-width:      80px       (w-20 in tailwind)
--nav-btn-size:   48px       (w-12 h-12)
--nav-btn-radius: 16px       (rounded-2xl)
```

Quando l'utente cambia accent, la nav-bg diventa l'accent color e il bottone attivo resta bianco.

---

## 2. Layout Globale

```
┌──────┬──────────────────────────────────────────┐
│      │  Content area (rounded-l-24px)            │
│ Nav  │  ┌──────────┬─────────────────────────┐  │
│ 80px │  │ Subnav   │  Main                    │  │
│      │  │ (opt.)   │                          │  │
│      │  │ 288px    │                          │  │
│      │  └──────────┴─────────────────────────┘  │
└──────┴──────────────────────────────────────────┘
```

- **Nav**: icon bar verticale, sfondo scuro/accent, fixed left
- **Content wrapper**: `flex-1`, background `--bg`, `border-radius: 24px 0 0 24px`, `box-shadow` inset da sinistra
- **Subnav**: opzionale per pagina (chat ha conversation list, tools ha tool list)
  - Border dashed a destra (come nel reference)
  - Collapsible con animazione

---

## 3. Componenti Chiave

### Nav bar (sidebar attuale → icon bar)
- Logo in alto (Homun logo invariato)
- Icone principali: Chat, Dashboard, Tools, E-Stop
- In basso: Account, Settings, Logout
- Hover: `bg-white/10`, scale 1.05
- Active: `bg-white text-black rounded-2xl shadow-sm`
- Subnavs appaiono come pannello adiacente (gia cosi)

### Content Cards
- `bg-white rounded-2xl border border-gray-200 shadow-sm`
- No ombre pesanti, solo `box-shadow: 0 1px 3px rgba(0,0,0,0.04)`

### Input areas (chat, search, forms)
- `bg-white/95 backdrop-blur-xl rounded-3xl border border-gray-200/80`
- `shadow-[0_20px_50px_-15px_rgba(0,0,0,0.1)]`
- Focus: `ring-1 ring-black/10` (sottile)

### Buttons
- Primary: `bg-black text-white rounded-full` (o rounded-xl per rettangolari)
- Ghost: `bg-transparent hover:bg-gray-100 text-gray-500`
- Danger: `text-red-500 hover:bg-red-50`

### Badges / Pills
- `text-xs font-semibold px-3 py-1.5 bg-white border rounded-full shadow-sm`
- Status dot: `w-2 h-2 rounded-full bg-emerald-500 animate-pulse`

### Typography scale
- Page title: `text-3xl md:text-4xl font-light tracking-tight`
- Section header: `text-[10px] font-bold tracking-[0.2em] uppercase text-gray-400`
- Body: `text-[15px] leading-relaxed`
- Meta: `text-[11px] font-mono uppercase tracking-wider text-gray-400`

---

## 4. Background Textures (Settings)

Salvate in `localStorage` come `homun-bg-texture`. Applicate al `<main>` content area.

| ID | Nome | Descrizione |
|----|------|-------------|
| `none` | Nessuno | Sfondo piatto `--bg` |
| `noise` | Carta | fractalNoise SVG, opacity 0.05 |
| `hatch` | Tratteggio | Linee diagonali 45deg, opacity 0.035 |
| `waves` | Onde | Sinusoidi SVG, opacity 0.05 |
| `grid` | Carta millimetrata | Griglia 20x20px, opacity 0.04 |
| `dots` | Puntini | Dot grid 16x16px, opacity 0.04 |
| `custom` | Custom | URL o emoji pattern (futuro) |

Tutti i pattern sono grigio chiaro `stroke-opacity` / `fill-opacity` basso.

---

## 5. Pagine — Impatto per pagina

| Pagina | Layout change | Note |
|--------|--------------|-------|
| **Chat** | Alto | Floating input, editorial messages, conversation subnav con dashed border |
| **Dashboard** | Medio | Card grid, stats pills, status dots |
| **Automations** | Medio | Master-detail con subnav |
| **Workflows** | Medio | Simile automations |
| **Skills** | Medio | Card grid marketplace |
| **MCP** | Medio | Card grid + OAuth flows |
| **Memory** | Basso | Lista + editor |
| **Knowledge** | Basso | Upload + search |
| **Vault** | Basso | Lista secrets |
| **Approvals** | Basso | Lista items |
| **Account** | Basso | Form |
| **Appearance** | Alto | Aggiungere texture picker |
| **Setup** | Basso | Form wizard |
| **Channels** | Medio | Card grid |
| **Browser** | Basso | Config form |
| **File Access** | Basso | Lista paths |
| **Shell** | Basso | Terminal |
| **Sandbox** | Basso | Config form |
| **Logs** | Basso | Streaming list |
| **Maintenance** | Basso | DB operations |

---

## 6. Piano Esecuzione

### Fase 1 — Foundation ✅ DONE (2026-03-17)
1. **Ridefinire i token CSS** in `:root` (nuova palette light/dark)
2. **Riscrivere il layout CSS** della nav (icon bar scura, rounded content area)
3. **Aggiornare `sidebar()` HTML** in pages.rs per matchare
4. **CSS dei componenti base**: buttons, cards, inputs, badges, typography
5. **Verificare**: tutte le pagine devono renderizzare senza breaking

### Fase 2 — Chat page ✅ DONE (2026-03-17)
1. Riscrivere chat template HTML per layout editorial
2. CSS chat: floating input, message cards, conversation sidebar
3. Adattare chat.js per nuovi selettori/classi

### Fase 3 — Dashboard + Content pages ✅ DONE (2026-03-17)
1. Dashboard: stat cards (28px values, r-2xl, border-subtle), section headers (13px sans, editorial)
2. Skills, Providers, Channels, MCP: card layouts (r-xl, border-subtle, hover lift)
3. Forms: labels (12px sans, no uppercase), inputs (r-xl, 14px), buttons (pill primary/accent)
4. Generic cards/sections: consistent editorial tokens

### Fase 4 — Polish ✅ DONE (2026-03-17)
1. Appearance page: texture picker (6 options: none, paper, dots, grid, hatch, waves)
2. Responsive check (375, 768, 1024, 1280) — verified all breakpoints
3. Dark mode verification — dashboard, chat, appearance verified
4. Accent color verification (nav bg cambia con accent) — preset + custom hex verified

---

## 7. Decisioni Tecniche

- **No Tailwind**: restiamo CSS custom con variabili. Motivi:
  - Binary embeds CSS (rust-embed), no CDN
  - Accent picker gia funziona con CSS vars
  - 9K+ righe di CSS esistente — migrazione a Tailwind sarebbe distruttiva
  - CSS vars + custom classes = stessa flessibilita, zero dipendenze

- **Plus Jakarta Sans**: gia importato, resta il font principale

- **Logo Homun**: invariato, posizionato in alto nella nav bar

- **Backwards compatibility**: le classi CSS cambiano nome gradualmente.
  Vecchie classi restano come alias durante la transizione.
